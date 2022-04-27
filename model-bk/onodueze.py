import pandas as pd
import numpy as np
import re
from tqdm import tqdm, trange
import math
from sklearn.model_selection import train_test_split
from tensorflow.python.keras.preprocessing.sequence import pad_sequences
import plotly.figure_factory as ff
from sklearn.metrics import confusion_matrix, roc_curve, auc
import csv
import plotly.express as px
import pickle
import tensorflow as tf
from tensorflow.keras import layers
from sklearn.metrics import classification_report
from sklearn.metrics import roc_auc_score
import struct
from tensorflow.keras.activations import softmax
from tensorflow.keras.layers import Input, Conv2D, MaxPooling2D, Flatten, Dense, Lambda, Embedding, Multiply, GRU, Dropout, LayerNormalization, LSTMCell, GRUCell, LSTM
from tensorflow.keras.layers import Input, Conv1D, Conv2D, MaxPooling2D, Flatten, Dense, Lambda, Embedding, Multiply, GRU, Dropout, LayerNormalization, LSTMCell, GRUCell, Bidirectional
from tensorflow.keras.models import Sequential, Model
from tensorflow.keras import layers, losses, optimizers, metrics, Model
import tensorflow as tf
import math
import numpy as np
import keras
from tensorflow.python.ops import array_ops
from tensorflow.keras.layers import Layer, GRUCell, GRU, RNN, Flatten, LSTMCell
from tensorflow.python.keras.layers.recurrent import _generate_zero_filled_state_for_cell
from sklearn.ensemble import IsolationForest
from sklearn.svm import OneClassSVM
from sklearn.covariance import MinCovDet
from sklearn.neighbors import LocalOutlierFactor
import xgboost as xgb
import timeit

def prepare(dataset, size=5):
    # (session, time, word, parity_error, attack)
    sessions = set([row[0] for row in dataset])

    # empty output array
    xs = []
    ys_anomaly = []
    ys_attack = []
    # loop through each word sent on the bus
    for s in sessions:
        words = [row for row in dataset if row[0] == s]
        for i in range(len(words)):
            win = words[max(0, i-size+1):i+1]
            if len(win) == size:
                # unpack integer into bytes
                x = [v for row in win for v in list(struct.pack(">I", row[2]))]
                # attack label 
                y = max([row[-1] for row in win])
                xs.append(x)
                ys_anomaly.append(y != 0)
                ys_attack.append(y)
    return xs, (ys_anomaly, ys_attack)


                  
def eval_onodueze(training_set, testing_sets, model='BLSTM', total_attacks=11):
    print('preparing data')
    xs, ys = prepare(training_set)
    if model == 'BLSTM':
        input_dim = 256 # maximum byte 
        embedding_size = 8 

        inp = Input( shape=(None,), dtype='int64')
        emb = Embedding(input_dim, embedding_size)(inp)

        lstm1 = Bidirectional(LSTM(128, return_sequences=True))(emb) #[None,40,8]
        lstm2 = Bidirectional(LSTM(128, return_sequences=True))(lstm1) #[None,32]
        lstm3 = Bidirectional(LSTM(128))(lstm2)

        dense = Dense(128, activation='relu')(lstm3)
        out_anomaly = Dense(1, activation='sigmoid', name='anomaly')(dense)

        model = tf.keras.Model(
            inputs=inp, 
            outputs=out_anomaly#(out_anomaly, out_misuse)
        )
        model.compile(
            loss='binary_crossentropy', 
            optimizer='adam',
            metrics=['binary_accuracy', 'AUC']
        )
        print(model.summary())
        
        print('start training', len(xs))
        print(xs[0], ys[0][0])
        model.fit(xs,
                  ys[0],
                  epochs=1,
                  batch_size=64,
                  verbose=1,
                  # validation_split = 0.66
                 )
        print('start testing')
        predictions = []
        for test in testing_sets:
            start = timeit.default_timer()
            xs, ys = prepare(test)
            predict_matrix = model.predict(xs)
            stop = timeit.default_timer()
            predictions.append((ys, predict_matrix, stop - start)) 
        return model, predictions 
    
    print('start training', model)
    if model == 'IsolationForest':
        model = IsolationForest(contamination=0.4, verbose=1)
        model.fit(xs, ys)
    if model == 'LOF':
        model = LocalOutlierFactor(contamination=0.1, algorithm='kd_tree', novelty=True)
        model.fit(xs, ys)
    if model == 'MCD':
        model = MinCovDet()
        model.fit(xs, ys)
    if model == 'XGBoost':
        data_xg = xgb.DMatrix(xs, label=ys)
        params = {
            "max_depth":"5",
            "eta":"0.2",
            "gamma":"4",
            "min_child_weight":"6",
            "subsample":"0.8",
            "verbosity":"1",
            "objective":"binary:logistic",
            "eval_metric": "error"
        }

        model = xgb.train(params=params, dtrain=data_xg)
    print('start testing', model.__class__.__name__)
    predictions = []
    for test in testing_sets:
        start = timeit.default_timer()
        xs, ys = prepare(test)
        if hasattr(model, 'score'):
            predict_matrix = model.score(xs)
        else:
            predict_matrix = model.predict(xs)
        stop = timeit.default_timer()
        predictions.append((ys, predict_matrix, stop - start)) 
    return model, predictions 
    
    
