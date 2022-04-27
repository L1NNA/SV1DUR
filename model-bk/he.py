from tqdm.notebook import tqdm
import pandas as pd
import os

from csv import reader

import random

# All of the columns used to represent a command word
COLUMN_NAMES = ['timestamp', 'RT_address', 'sub_address', 'mode_code', 'time_interval', 'protocol', 'fake']

FEATURE_LABELS = COLUMN_NAMES[1:-1]
CLASS_LABELS = COLUMN_NAMES[0:]

class dataCollector():
    def __init__(self, dataset):
        # list of (session, time, word, parity_error, attack)
        self.dataset = dataset

    def collectData(self):
        data = pd.DataFrame(self.dataset, columns =['session', 'timestamp', 'word', 'parity', 'attack'])
        # data = pd.read_csv(path, names=['timestamp', 'word', 'parity', 'attack'])
        
        data['protocol'] = '1553'
        data['time_interval'] = data['timestamp'].diff()

        data = data[1:]

        # Discard data messages (Sync Bits (0-2) = 0b000)
        data = data[data['word'] & 7 != 0]
        word = data['word'].map(lambda x: x >> 3)

        # Discard status messages (Instrumentation Bit (7) Clear and Reserved Bits (9-11) Clear)
        data = data[word & 0x740 != 0x40]

        data['RT_address']  = word & 0b0000_0000_0001_1111 # 001F
        data['RT_bit']      = word & 0b0000_0000_0010_0000 # 0020
        data['sub_address'] = word & 0b0000_0111_1100_0000 # 07C0
        data['mode_code']   = word & 0b1111_1000_0000_0000 # F800
        
        data['RT_bit'] = data['RT_bit'].map(lambda x: x >> 5)
        data['sub_address'] = data['sub_address'].map(lambda x: x >> 6)
        data['mode_code'] = data['mode_code'].map(lambda x: x >> 11)

        data['fake'] = data['attack'] != 0
            
        return data[COLUMN_NAMES]
    
    
# Pandas is annoying
import warnings
#warnings.filterwarnings("ignore", category=FutureWarning) 
#warnings.stacklevel = 5

import pandas as pd
import numpy as np
import random
from enum import Enum
#import logging as log
import os
from sklearn.model_selection import train_test_split
from sklearn.naive_bayes import GaussianNB
from sklearn import metrics
from sklearn.metrics import classification_report, confusion_matrix
from sklearn.metrics import f1_score, recall_score, precision_score, fbeta_score, roc_auc_score
import joblib
from sklearn.model_selection import cross_val_score
from sklearn.model_selection import ShuffleSplit
from .dataUtilities import dataCollector
import matplotlib.pyplot as plt
import itertools

from tqdm.notebook import tqdm
tqdm.pandas()


array_type = type(np.ndarray([]))

class Mode(Enum):
    PREPARE = 1
    MONITOR = 2    
    
class system_He():
    def __init__(self, dataset, sysMode=Mode.PREPARE):
        self.id="He et al."
        self.year=2020
        self.sysMode : Mode = sysMode
        self.giniThresholds={"RT_address":0.7,"sub_address":0.7,"mode_code":0.7,"time_interval":0.7,"protocol":0.7}
        #self.giniThresholds={"RT_address":0.1,"sub_address":0.2,"mode_code":0.3,"time_interval":0.6,"protocol":0.4}        
        self.modelFilePath="models/model_He.pkl"
        self.normal_beh_spec= pd.DataFrame()        
        self.dataset = dataset
            
    def prepareData(self):
        
        dc=dataCollector(self.dataset)
        print(f'Using data from: {self.path}')
        dc.collectData()
        self.data=dc.cmd_data
        dataset=dc.cmd_data     
        self.labels = dataset['fake']
        self.features=dataset[['RT_address', 'sub_address', 'mode_code','time_interval','protocol']]

        #self.system_df, self.traffic_df, self.system_label, self.traffic_label = train_test_split(self.features, self.labels, test_size=0.9, random_state=50)        
        self.system_df, self.traffic_df, self.system_label, self.traffic_label = train_test_split(self.features, self.labels, test_size=0.9, random_state=50)        
        
        self.system_data=dataset[dataset['fake']==0][['RT_address', 'sub_address', 'mode_code','time_interval','protocol']].sample(frac = 0.1,random_state=1) 
      
       
    def gini(self,array):
        if not isinstance(array,np.ndarray):
            array=array.to_numpy()
        #array = array.flatten()
        unique_elements, counts_elements = np.unique(array, return_counts=True)
        gini_D=array.size
        sum=0

        for i in range (unique_elements.size):
            gini_c=counts_elements[i]
            sum+=(gini_c/gini_D)**2

        gini_index=1-sum

        return gini_index
    
    def generate_beh_spec(self,df_gini,df=None):
        
        if df is None:
            df=self.system_data
        
        df_GTh=pd.Series(self.giniThresholds)
        
        self.df_gini=df_gini
        self.df_GTh=df_GTh
        
        condition=df_gini > df_GTh 
        gini_check = df_gini[condition]
       
        self.beh_spec = df.apply(lambda x: x.unique(), result_type = 'reduce')

        for i in range (gini_check.size):
            att_name=gini_check.index[i]
            if att_name == "time_interval":
                min=df[att_name].min()
                max=df[att_name].max()                
                self.beh_spec[att_name]=list([min,max])
            else:
                self.beh_spec[att_name]=df[att_name].unique()

        self.normal_beh_spec= self.normal_beh_spec.append(self.beh_spec,ignore_index=True)
        self.normal_beh_spec=self.normal_beh_spec.astype({"RT_address":np.int64})
        
        
    def comp_gini(self,group=False):
        df_gini=[]
        if not group:
            df_gini=self.system_data.apply(lambda x: self.gini(x))
            self.generate_beh_spec(df_gini)
            
        else:
            df_gp=self.system_data.groupby("RT_address")
            
            for k, gp in df_gp:             
                df_gini=gp.apply(lambda x: self.gini(x))
                self.generate_beh_spec(df_gini,gp)                

        
    def phase_one(self):

        print('Preparing Data...')
        self.prepareData()   #offline data
              
        print('Comparing Gini Impurity...')
        df_gini=self.comp_gini(group=True)
        
        print('Training...')
        self.ml_train()
        
            
    def check_spec(self,traffic):
        
        self.traffic=traffic
        
        
        rhs=traffic['RT_address']
        
        lhs=self.normal_beh_spec['RT_address']
        
        check = lhs==rhs
        
        traffic_normal_spec=self.normal_beh_spec[check]
        
        result=True
        
        if traffic_normal_spec.empty:
            return False
        
        c1 = traffic.sub_address in list(traffic_normal_spec.sub_address)[0]
        c2 = traffic.mode_code in list(traffic_normal_spec.mode_code)[0]
        c3 = traffic.protocol in list(traffic_normal_spec.protocol)[0]
        
        time_interval = list(traffic_normal_spec.time_interval)[0]
        lower = time_interval[0]
        upper = time_interval[1] if len(time_interval) == 2 else lower

        c4 = traffic.time_interval >= lower
        c5 = traffic.time_interval<= upper
        result=(c1 and c2 and c3 and c4 and c5)        
        
        if not (result):
            result=self.ml_test(traffic)
     
       
        return result
            
    def encode_data(self,data):
                
        data["protocol"] = data["protocol"].astype('category')
        data["protocol"] = data["protocol"].cat.codes
        
        return data

    def ml_train(self):        
   
        if not (os.path.isfile(self.modelFilePath) and self.sysMode==Mode.PREPARE):
        
            print("Phase 1: preparing the system and training GaussianNB model")

            x_train, x_test, y_train, y_test = train_test_split(self.system_df, self.system_label, test_size=0.30, random_state=101)
            
            nbc_model = GaussianNB()
            
            nbc_model.fit(x_train.values, y_train.values)
            y_pred = nbc_model.predict(x_test.values)
            
            self.evaluate_model(y_test, y_pred)
            
            print("NBC model trained and saved")
            joblib.dump(nbc_model, self.modelFilePath)            
      
    def final_evaluate(self):
        X=self.features
        y=self.labels
        
        #nbc_model = GaussianNB()
        
        myCV = ShuffleSplit(n_splits=5, test_size=0.7, random_state=0)
        
        recall = cross_val_score(self.nbc_model, X, y, cv=myCV, scoring='recall_macro')
        print('Recall', np.mean(recall), recall)
        precision = cross_val_score(self.nbc_model, X, y, cv=myCV, scoring='precision_macro')
        print('Precision', np.mean(precision), precision)
        f1 = cross_val_score(self.nbc_model, X, y, cv=myCV, scoring='f1_macro')
        print('F1', np.mean(f1), f1)
        
    
    def evaluate_model (self,y_actual,y_pred):        
              
        cnf_matrix = confusion_matrix(y_actual, y_pred, labels=[0,1])        
        print("f1-score (macro): ",f1_score(y_actual, y_pred, average='macro')*100,"%")
        print("recall (macro): ",recall_score(y_actual, y_pred, average='macro')*100,"%")
        print("precision (macro): ",precision_score(y_actual, y_pred, average='macro')*100,"%")
        #print("f-beta (macro): ",fbeta_score(y_actual, y_pred, average='macro')*100,"%")
        print("ROCAUC (macro): ",roc_auc_score(y_actual, y_pred, average='macro')*100,"%")
        
        print("f1-score (weighted): ",f1_score(y_actual, y_pred, average='weighted')*100,"%")
        print("recall (weighted): ",recall_score(y_actual, y_pred, average='weighted')*100,"%")
        print("precision (weighted): ",precision_score(y_actual, y_pred, average='weighted')*100,"%")
        #print("f-beta (weighted): ",fbeta_score(y_actual, y_pred, average='weighted')*100,"%")
        print("ROCAUC (macro): ",roc_auc_score(y_actual, y_pred, average='weighted')*100,"%")

        # Plot non-normalized confusion matrix
        plt.figure()
        self.plot_confusion_matrix(cnf_matrix, classes=['Benign(0)','Attack(1)'],normalize= False,title="Confusion matrix")
        
    def plot_confusion_matrix(self,cm, classes,
                          normalize=False,
                          title='Confusion matrix',
                          cmap=plt.cm.Blues):
        
        if normalize:
            cm = cm.astype('float') / cm.sum(axis=1)[:, np.newaxis]
            
        plt.imshow(cm, interpolation='nearest', cmap=cmap)
        plt.title(title)
        plt.colorbar()
        tick_marks = np.arange(len(classes))
        plt.xticks(tick_marks, classes, rotation=45)
        plt.yticks(tick_marks, classes)

        fmt = '.2f' if normalize else 'd'
        thresh = cm.max() / 2.
        for i, j in itertools.product(range(cm.shape[0]), range(cm.shape[1])):
            plt.text(j, i, format(cm[i, j], fmt),
                     horizontalalignment="center",
                     color="white" if cm[i, j] > thresh else "black")

        plt.tight_layout()
        plt.ylabel('True label')
        plt.xlabel('Predicted label')
        
    def ml_test(self,traffic):
        
        xTest=traffic.values.reshape(1,-1)           
        
        result = self.nbc_model.predict(xTest)
                        
        label=True if result == 1 else False
        
        return label
        
    def phase_two(self):
        self.sysMode = Mode.MONITOR
       
        print("Phase 2: loading trained model and start monitoring")

        self.nbc_model = joblib.load(self.modelFilePath)
        
        print(self.traffic_df)# = self.traffic_df
        print(self.traffic_label)
        
#        self.predict_label=self.traffic_df.progress_apply(lambda x:self.check_spec(x),axis=1)  
#        self.evaluate_model(self.traffic_label,self.predict_label.astype(int))        

        
    def run(self):
        #log.info('He et al. system: training...')        
        self.phase_one()
        self.phase_two()
        print("done")