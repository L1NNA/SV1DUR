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
from tensorflow.keras.layers import Input, Conv2D, MaxPooling2D, Flatten, Dense, Lambda, Embedding, Multiply, GRU, Dropout, LayerNormalization, LSTMCell, GRUCell
from tensorflow.keras.layers import Input, Conv1D, Conv2D, MaxPooling2D, Flatten, Dense, Lambda, Embedding, Multiply, GRU, Dropout, LayerNormalization, LSTMCell, GRUCell
from tensorflow.keras.models import Sequential, Model
from tensorflow.keras import layers, losses, optimizers, metrics, Model
import tensorflow as tf
import math
import numpy as np
import keras
from tensorflow.python.ops import array_ops
from recurrent import *
from tensorflow.keras.layers import Input, Conv2D, MaxPooling2D, Flatten, Dense, Lambda, Embedding, Multiply, GRU, Dropout, LayerNormalization, LSTMCell, GRUCell
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
                         
                         
                        
def _generate_zero_filled_state_for_cell(cell, inputs, batch_size, dtype):
    if inputs is not None:
        batch_size = array_ops.shape(inputs)[0]
        dtype = inputs.dtype
    return _generate_zero_filled_state(batch_size, cell.state_size, dtype)


def _generate_zero_filled_state(batch_size_tensor, state_size, dtype):
    if batch_size_tensor is None or dtype is None:
        raise ValueError(
        'batch_size and dtype cannot be None while constructing initial state: '
        'batch_size={}, dtype={}'.format(batch_size_tensor, dtype))

    def create_zeros(unnested_state_size):
        flat_dims = tensor_shape.TensorShape(unnested_state_size).as_list()
        init_state_size = [batch_size_tensor] + flat_dims
        return array_ops.zeros(init_state_size, dtype=dtype)

    if nest.is_nested(state_size):
        return nest.map_structure(create_zeros, state_size)
    else:
        return create_zeros(state_size)


'''
Modern Tensorflow Implementation of skip-LSTM: https://arxiv.org/abs/1708.06834
'''
class skipLSTM(keras.layers.Layer):
    '''
    num_cells: number of nodes in the layer
    name: name for the layer
    '''
    def __init__(self, num_cells=100, name='lstm', **kwargs):
        super(skipLSTM, self).__init__(name=name, **kwargs)
        self.num_cells = num_cells
        self.rnn_cell = LSTMCell(self.num_cells)
        self.dense = Dense(1, use_bias=True, activation='sigmoid')
        
    # Build the basic cells. Done automatically for dense layer
    def build(self, input_shape):
        if isinstance(input_shape, list):
            input_shape = input_shape[0]
        if not self.rnn_cell.built:
            with keras.backend.name_scope(self.rnn_cell.name):
                self.rnn_cell.build(input_shape)
                self.rnn_cell.built = True
    
        self.built = True
    
    # Used when this layer is part of a network
    # o_inputs is of shape [batch, window size, embedding_dim]
    def call(self, o_inputs, **kwargs):
        # o_inputs is of shape [batch, window size, embedding_dim]
        win_size = tf.shape(o_inputs)[1]
        batch_size = tf.shape(o_inputs)[0]
        
        results = tf.TensorArray(dtype=tf.float32, size=win_size)
        
        '''
        This function is applied to each timestamp of the sequence for the node
        Inputs/Outputs:
            t: current timestamp
            ut: state update gate for current timestamp
            state:  array containing: [ht_1, ct_1]: hidden and candidates from previous timestamp
            res: stacking array containing node output values
        '''
        def _step(t, state, ut, res):
            
            # determine the state of the update gate: {0,1} (Eq 2)
            ut_gate = tf.round(ut)
            
            # generate output for current timestamp t (Eq 3)
            (out, state_n) = self.rnn_cell(o_inputs[:, t, :], state)

            # determine whether to actually update the state based on update gate (Eq 3)
            ht_n = tf.expand_dims(ut_gate, 1) * state[0] + tf.expand_dims(1 - ut_gate, 1) * state_n[0]
            ct_n = tf.expand_dims(ut_gate, 1) * state[1] + tf.expand_dims(1 - ut_gate, 1) * state_n[1]
            state_n = [ht_n, ct_n]
            
            # compute the change in update gate value based on hidden state (Eq 4)
            # concatenate the hidden and candidate so it can be passed
            delta_ut = tf.squeeze(self.dense(tf.concat([ht_n, ct_n], 1)), 1)
            
            # compute the value of the update gate for the next timestamp (Eq 5)
            ut_n = ut_gate * delta_ut + (1 - ut_gate) * (ut + tf.minimum(delta_ut, 1-ut))
            
            # write node output to index t of array res (returned the updated res array)
            res_updated = res.write(t, out)
            
            return t+1, state_n, ut_n, res_updated
        
        # generate initial weights of all 0
        state0 = _generate_zero_filled_state_for_cell(self.rnn_cell, o_inputs, None, None)
        u0 = tf.ones(batch_size, dtype=tf.float32)
        
        '''
        Loop through timestamps and return the pile of node outputs
        Inputs:
            Stop condition on while loop
            Step function applied at each iteration
            Initial values in loop
        '''
        *_, final_res = tf.while_loop(
            lambda t, *_: t < win_size, 
            _step,
            (0, state0, u0, results)
        )
        
        final_res = final_res.stack()
        # [time, batch, cell_dim]
        final_res = tf.transpose(final_res, [1, 0, 2])
        # [batch, time, cell_dim]
        return final_res
    
'''
Modern Tensorflow Implementation of skip-GRU: https://arxiv.org/abs/1708.06834
'''
class skipGRU(keras.layers.Layer):
    '''
    num_cells: number of nodes in the layer
    name: name for the layer
    '''
    def __init__(self, num_cells=100, name='skipGRU', **kwargs):
        super(skipGRU, self).__init__(name=name, **kwargs)
        self.num_cells = num_cells
        self.rnn_cell = GRUCell(self.num_cells)
        self.dense0 = Dense(1, use_bias=True, activation='sigmoid')
    
    # Build the basic cells. Done automatically for dense layer
    def build(self, input_shape):
        if isinstance(input_shape, list):
            input_shape = input_shape[0]
        if not self.rnn_cell.built:
            with keras.backend.name_scope(self.rnn_cell.name):
                self.rnn_cell.build(input_shape)
                self.rnn_cell.built = True
        
        self.built = True
    
    # Used when this layer is part of a network
    # o_inputs is of shape [batch, window size, embedding_dim]
    def call(self, o_inputs, **kwargs):
        
        # get the window size and batch size
        win_size = tf.shape(o_inputs)[1]
        batch_size = tf.shape(o_inputs)[0]
        
        # setup the return array which contains the output values for the layer
        results = tf.TensorArray(dtype=tf.float32, size=win_size)
        
        '''
        This function is applied to each timestamp of the sequence for the node
        Inputs/Outputs:
            t: current timestamp
            ut: state update gate for current timestamp
            ht_1:  hidden state from previous timestamp
            res: stacking array containing node output values
        '''
        def _step(t, ht_1, ut, res):
        
            # determine the state of the update gate: {0,1} (Eq 2)
            ut_gate = tf.round(ut)
            
            
            # generate output for current timestamp t (Eq 3)
            ht, _ = self.rnn_cell(o_inputs[:, t, :], ht_1)
            
            # determine whether to actually update the state based on update gate (Eq 3)
            ht = tf.expand_dims(ut_gate, 1) * ht + tf.expand_dims(1 - ut_gate, 1) * ht_1
            
            # compute the change in update gate value based on hidden state (Eq 4)
            delta_ut = tf.squeeze(self.dense0(ht), axis=1)
            
            # compute the value of the update gate for the next timestamp (Eq 5)
            ut_n = ut_gate * delta_ut + (1 - ut_gate) * (ut + tf.minimum(delta_ut, 1-ut))

            # write ht to index t of array res (returned the updated res array)
            res_updated = res.write(t, ht)
            
            return t+1, ht, ut_n, res_updated
        
        # generate initial weights of all 0
        h0 = _generate_zero_filled_state_for_cell(self.rnn_cell, o_inputs, None, None)
        u0 = tf.ones(batch_size, dtype=tf.float32)
        
        '''
        Loop through timestamps and return the pile of node outputs
        Inputs:
            Stop condition on while loop
            Step function applied at each iteration
            Initial values in loop
        '''
        *_, final_res = tf.while_loop(
            lambda t, *_: t < win_size, 
            _step,
            (0, h0, u0, results)
        )
        
        # flip batch size and time so it lines up with the rest of the network
        final_res = final_res.stack()
        # [time, batch, cell_dim]
        final_res = tf.transpose(final_res, [1, 0, 2])
        # [batch, time, cell_dim]
        return final_res
        

# leap-LSTM
def sample_gumbel(shape, eps=1e-20): 
    """Sample from Gumbel(0, 1)"""
    U = tf.random.uniform(shape,minval=0,maxval=1)
    return -tf.math.log(-tf.math.log(U + eps) + eps)
    
    
def gumbel_softmax_sample(logits, temperature=1e-5): 
    """ Draw a sample from the Gumbel-Softmax distribution"""
    y = logits + sample_gumbel(tf.shape(logits))
    return tf.nn.softmax(y / temperature)


'''
Modern Tensorflow Implementation of leap-LSTM: 
'''


class leapLSTM(keras.layers.Layer):
    '''
    num_cells: number of nodes in the layer
    name: name for the layer
    '''

    def __init__(self, num_cells=100, name='lstm', small_cell_size=10, **kwargs):
        super(leapLSTM, self).__init__(name=name, **kwargs)
        self.num_cells = num_cells
        self.rnn_cell = LSTMCell(self.num_cells)
        self.dense0 = Dense(100, activation="relu", use_bias=True)
        self.dense1 = Dense(2, use_bias=True)

        self.conv1=Conv1D(60, 3, padding = 'same')
        self.conv2=Conv1D(60, 4, padding = 'same')
        self.conv3=Conv1D(60, 5, padding = 'same')
        self.rnn_rev=LSTM(10, return_sequences=True)  # p


    # Build the basic cells. Done automatically for dense layer
    def build(self, input_shape):
        if isinstance(input_shape, list):
            input_shape=input_shape[0]
        if not self.rnn_cell.built:
            with keras.backend.name_scope(self.rnn_cell.name):
                self.rnn_cell.build(input_shape)
                self.rnn_cell.built=True

        self.built=True

    # Used when this layer is part of a network
    # o_inputs is of shape [batch, window size, embedding_dim]
    def call(self, o_inputs, **kwargs):
        # o_inputs is of shape [batch, window size, embedding_dim]
        batch_size=tf.shape(o_inputs)[0]
        win_size=tf.shape(o_inputs)[1]
        enb_size=tf.shape(o_inputs)[2]

        results=tf.TensorArray(dtype = tf.float32, size = win_size)


        conved=tf.concat([
            self.conv1(o_inputs),
            self.conv2(o_inputs),
            self.conv3(o_inputs)],
            axis = -1)
        # shape = [batch, time, 60*3]


        rev_lstm=self.rnn_rev(
            tf.reverse(
                o_inputs, axis=[1]
            )
        )
        # shape = [batch, time, 10]

        f_all=tf.concat([conved, rev_lstm], axis = -1)
        # shape = [batch, time, 190]

        h_end=tf.zeros(shape = [batch_size, 190])
        # shape = [batch, 190]
        
        
        f_all=tf.concat([
            f_all, 
            tf.expand_dims(h_end, axis=1)
        ], axis=1)
        # shape = [batch, time+1, 190]

        '''
        This function is applied to each timestamp of the sequence for the node
        Inputs/Outputs:
            t: current timestamp
            state:  array containing: [ht_1, ct_1]: hidden and candidates from previous timestamp
            res: stacking array containing node output values
        '''
        def _step(t, ht_1, res):

            x_t=o_inputs[:, t, :]

            ff=f_all[:, t+1, :]
   
            
            pi_t = self.dense1(
                self.dense0(
                    tf.concat([ff, x_t], axis=-1)
                )
            )
            
            d_t = gumbel_softmax_sample(pi_t) # tf tensor
            # [None, 2] 2: (keep, skip)
            
            
            # generate output for current timestamp t (Eq 3)
            (out, ht_candidate) = self.rnn_cell(x_t, ht_1)
            
            
            
            # make d_t rank 3 tensor so after slicing it will become rank-2
            d_t = tf.expand_dims(d_t, [1])
            # [None, 1, 2]
            
            ht = [
                d_t[:,:,0] * ht_candidate[0] + d_t[:,:,1] * ht_1[0], # combing keep & skip for lstm state h (element 0)
                d_t[:,:,0] * ht_candidate[1] + d_t[:,:,1] * ht_1[1], # combing keep & skip for lstm state c (element 1)
            ]
            
            # ht = tf.equal(d_t, 0) * ht_candidate + (1 - tf.equal(d_t, 0)) * ht_1
            
            # write node output to index t of array res (returned the updated res array)
            res_updated = res.write(t, out)
            
            return t+1, ht, res_updated
        
        # inital state
        state0 = _generate_zero_filled_state_for_cell(self.rnn_cell, o_inputs, None, None)
        
        *_, final_res = tf.while_loop(
            lambda t, *_: t < win_size, 
            _step,
            (0, state0, results)
        )
        
        
        final_res = final_res.stack()
        # [time, batch, cell_dim]
        final_res = tf.transpose(final_res, [1, 0, 2])
        # [batch, time, cell_dim]
        return final_res
                         
                         
                   
def eval_rnn(training_set, testing_sets, rnn_layer=GRU, total_attacks=11):
    input_dim = 256 # maximum byte 
    embedding_size = 8 

    inp = Input( shape=(None,), dtype='int64')
    emb = Embedding(input_dim, embedding_size)(inp)
    
    rnn = rnn_layer(128)(emb)
    
    # squish into (None, 64) from (None, None, 64)
    if len(rnn.shape) == 3:
        reduce = tf.reduce_mean(rnn, axis=1)
    else:
        reduce = rnn
    
    dense = Dense(128, activation='relu')(reduce)
    out_anomaly = Dense(1, activation='sigmoid', name='anomaly')(dense)
    #out_misuse = Dense(total_attacks, activation='softmax', name='misuse')(dense)
 
    model = tf.keras.Model(
        inputs=inp, 
        outputs=out_anomaly#(out_anomaly, out_misuse)
    )
    model.compile(
        loss='binary_crossentropy', #['binary_crossentropy', 'sparse_categorical_crossentropy'], 
        optimizer='adam',
        metrics=['binary_accuracy', 'AUC']#[['binary_accuracy', 'AUC'], ['SparseCategoricalAccuracy']]
    )
    # print(model.summary())
    print('preparing data')
    xs, ys = prepare(training_set)
    print('start training', len(xs))
    print(xs[0], ys[0][0])
    model.fit(xs,
              ys[0],
              epochs=1,
              batch_size=128,
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