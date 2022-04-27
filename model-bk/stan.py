import pandas as pd
import numpy as np
import pickle
import plotly.figure_factory as ff
from sklearn.metrics import confusion_matrix, roc_curve, auc
import plotly.express as px
import timeit

TIME_THRESHOLD = 40_000
ANOMALY_THRESHOLD = 1e-3  # Should be the minimum of [stateProb * transProb] for a given pair in the training set.
BENIGN = 1
ANOMALY = 0
current_model_name = 'Markov Chain'

masks = {
    'sync'          : 0b00000000000000000111,
    'address'       : 0b00000000000011111000,
    'errorbit'      : 0b00000000000100000000,
    'instrument'    : 0b00000000001000000000,
    'sr'            : 0b00000000010000000000,
    'reserved'      : 0b00000011100000000000,
    'brdcast'       : 0b00000100000000000000,
    'busy_bit'      : 0b00001000000000000000,
    'subsystem'     : 0b00010000000000000000,
    'bus_control'   : 0b00100000000000000000,
    'terminal_flag' : 0b01000000000000000000,
    'parity'        : 0b10000000000000000000,
    'tr'            : 0b00000000000100000000,
    'sub_address'   : 0b00000011110000000000,
    'mode'          : 0b00000011100000000000,
    'word_count'    : 0b01111100000000000000,
    'mode_code'     : 0b01111100000000000000,
    'data'          : 0b01111111111111111000,
}
shift = {
    'sync'          : 0,
    'address'       : 3,
    'errorbit'      : 8,
    'instrument'    : 9,
    'sr'            : 10,
    'reserved'      : 11,
    'brdcast'       : 14,
    'busy_bit'      : 15,
    'subsystem'     : 16,
    'bus_control'   : 17,
    'terminal_flag' : 18,
    'parity'        : 19,
    'tr'            : 8,
    'sub_address'   : 10,
    'mode'          : 11,
    'word_count'    : 14,
    'mode_code'     : 14,
    'data'          : 3,
}


def prepare_pairs(data_rows, save_after=False):
    """
    Read in a csv for use by the classifier
    """
    # (session, time, word, parity_error, attack)
    df = pd.DataFrame(data_rows, columns =['session', 'time', 'word', 'parity', 'attack'])
    df['data'] = (df['word'] & masks['sync']) == 0
    df['command'] = ((df['word'] & masks['instrument']) > 0) & (df['data'] == False)
    pairs = merge_pairs(df)
    return pairs

def merge_pairs(df):
    """
    Take command words and turn them into a pair to associate them with a single message.
    
    TODO: This currently doesn't grab the attacks correctly.  Must fix that before putting it into use.
    """
    commands = []
    df = df[df['command']==True]
    rcv_word = None
    for _, entry in df.iterrows():
        word = entry['word']
        if word & masks['tr'] == 0:
            rcv_time = entry['time']
            rcv_word = word
            rcv_word_count = (rcv_word & masks['word_count']) >> shift['word_count']
            label = entry['attack']
            if (rcv_word & masks['mode']) >> shift['mode'] != 2:
                mode = (rcv_word & masks['mode']) >> shift['mode']
                # BC Sending command
                destination = (rcv_word & masks['address']) >> shift['address']
                commands.append({'time': rcv_time, 
                                 'features': (None, destination, rcv_word_count, mode),
                                 'attack': label})
        elif rcv_word:
            trx_word = word
            if rcv_word & masks['word_count'] == trx_word & masks['word_count']:
                # This is a valid pair
                source = (trx_word & masks['address']) >> shift['address']
                destination = (rcv_word & masks['address']) >> shift['address']
                commands.append({'time': rcv_time, 
                                 'features': (source, destination, rcv_word_count, False),
                                 'attack': label})
    return pd.DataFrame(commands)

def extract_time_cycles(ts):
    """
    Determine whether an event is periodic or aperiodic.
    """
    td = {}
    for entry in ts:
        if entry['features'] not in td.keys():
            td[entry['features']] = []
            td[entry['features']].append(entry['time'])
        else:
            td[entry['features']].append(entry['time'])
    for key in td.keys():
        clusters = []
        td[key] = np.diff(td[key]).sort()
        k = 0
        clusters[k] = td[key][k]
        diffs = np.diff(td[key])
        for diff in diffs:
            if diff > TIME_THRESHOLD:
                k += 1
            clusters[k].append(diff)
        cycles[key] = []
        for i in range(k):
            cycles[key].append(np.mean(clusters[i]))
    return cycles

def count_transitions(df):
    """
    Count the number of occurences and transitions
    """
    occur = {}
    trans = {}
    previous_word = None
    # for _, entry in df.iterrows():
    for i in range(0, len(df)):
        entry = df.iloc[i]
        if entry['features'] not in occur.keys():
            occur[entry['features']] = 0
            trans[entry['features']] = {}
        occur[entry['features']] += 1
        if i != 0:
            previous_word = df.iloc[i-1]['features']
            if entry['features'] not in trans[previous_word].keys():
                trans[previous_word][entry['features']] = 0
            trans[previous_word][entry['features']] += 1
    return occur, trans

def accumulate_transitions(occur, trans):
    if type(occur) != list or type(trans) != list:
        return occur, trans
    keys = set()
    for occurrences in occur:
        keys = keys.union(set(occurrences.keys()))
    total_occur = {}
    total_trans = {}
    for key in keys:
        total_occur[key] = 0
        for occurrences in occur:
            if key in occurrences.keys():
                total_occur[key] += occurrences[key]
        total_trans[key] = {}
        for other_key in keys:
            val = 0
            for transitions in trans:
                if key in transitions and other_key in transitions[key].keys():
                    val += transitions[key][other_key]
            if val > 0:
                total_trans[key][other_key] = val
    return total_occur, total_trans


def transition_probabilities(occurrences, transitions):
    """
    Generate probability distributions for the states and transitions
    """
    state_prob = {}
    trans_prob = {}
    total_occurrences = sum(occurrences.values())
    for key in occurrences.keys():
        state_prob[key] = occurrences[key]/total_occurrences
        trans_prob[key] = {}
        for other_key in transitions[key].keys():
            trans_prob[key][other_key] = transitions[key][other_key] / occurrences[key]
    return state_prob, trans_prob

def model_score(word, previous_word, model):
    """
    Evaluate the score of a given transition
    """
    state_probability, transition_probability = model
    try:
        score = state_probability[previous_word] * transition_probability[previous_word][word]
    except:
        score = 0.0
    return score

def detect_anomaly(df, model, threshold=ANOMALY_THRESHOLD):
    """
    Provide labels and scores for all messages in the dataset.
    """
    labels = [BENIGN]
    scores = [1.0]
    for i in range(1, len(df)):
        try:
            score = model_score(df.iloc[i]['features'], df.iloc[i-1]['features'], model)
        except:
            score = 0.0
            print(f"i: {i}, features: {df.iloc[i]['features']}, attack: {df.iloc[i]['attack']}")
            print(f"i-1: {i-1}, features: {df.iloc[i-1]['features']}")
            raise
        scores.append(score)
        if score > threshold:
            labels.append(BENIGN)
            last_benign = i
        else:
            labels.append(ANOMALY)
            if df.iloc[i-1]['attack'] != 0:
                try:
                    score = model_score(df.iloc[i]['features'], df.iloc[last_benign]['features'], model)
                    scores[-1] = score
                except:
                    print(f"i: {i}, features: {df.iloc[i]['features']}, attack: {df.iloc[i]['attack']}")
                    print(f"last_benign: {last_benign}, features: {df.iloc[last_benign]['features']}")
                if score > threshold:
                    labels[-1] = BENIGN
                    last_benign = i
    return labels, scores



def eval_stan(training_set, testing_sets):
    # (session, time, word, parity_error, attack)
    # benign session as baseline: Flight Data YOW YUL Manual Flight_0
    baseline_entries = [r for r in training_set if r[0].endswith('_0')]
    other_entries = [r for r in training_set if not r[0].endswith('_0')]
    
    
    baseline_pairs = prepare_pairs(baseline_entries)
    other_baseline_pairs = prepare_pairs(other_entries)


    occurences, transitions = count_transitions(baseline_pairs)
    other_occurences, other_transitions = count_transitions(other_baseline_pairs)
    occurences, transitions = accumulate_transitions([occurences, other_occurences], [transitions, other_transitions])
    state_probability, transition_probability = transition_probabilities(occurences, transitions)
    model = (state_probability, transition_probability)


    THRESHOLD = 1.0
    for key in state_probability.keys():
        for other_key in transition_probability[key].keys():
            THRESHOLD = min(THRESHOLD, state_probability[key] * transition_probability[key][other_key])

            
    print('start testing')
    predictions = []
    for test in testing_sets:
        start = timeit.default_timer()
        fut_pairs = prepare_pairs(test)
        labels, scores = detect_anomaly(fut_pairs, model, threshold=THRESHOLD)
        anomaly_score = 1 - np.array(scores)
        stop = timeit.default_timer()
        predictions.append(
            # anomaly label, attack (misuse) label
            ((fut_pairs['attack'] != 0, fut_pairs['attack']),
            anomaly_score, stop - start)
        )

    return predictions