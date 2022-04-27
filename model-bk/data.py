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
from jvd.utils import grep_ext
from pathlib import Path
import os


def get_datasets(path='data/'):
    all_data = {}
    for session_folder_name in os.listdir(path):
        session_folder = os.path.join(path, session_folder_name)
        if os.path.isdir(session_folder) and not session_folder_name.startswith('.'):
            data = []
            cache = session_folder+'.pkl'
            if not os.path.exists(cache):
                # (session, time, word, parity_error, attack)
                for file in tqdm(grep_ext(session_folder, ext='.dat')):
                    with open(file) as rf:
                        for line in rf.readlines():
                            parts = [int(p.strip()) for p in line.split(',')]
                            data.append([Path(file).parent.name] + parts)
                with open(cache, 'wb') as file:
                    pickle.dump(data, file)
            else:
                with open(cache, 'rb') as file:
                    data = pickle.load(file)
            all_data[session_folder_name] = data
    return all_data



if __name__ == '__main__':
    all_data = get_datasets()
    