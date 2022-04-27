from __future__ import division
from __future__ import print_function

import os
#os.environ["CUDA_VISIBLE_DEVICES"] = "1"
import time
import argparse
import numpy as np
from tqdm import tqdm

import torch
import torch.nn as nn
import torch.nn.functional as F
import torch.optim as optim
import torch_geometric

from ignn_g.utils import accuracy, clip_gradient, l_1_penalty, get_spectral_rad
from ignn_g.models import IGNN
import networkx as nx
import struct
from torch_geometric.data import DataLoader
from scipy.sparse import csr_matrix
from ignn_g.normalization import fetch_normalization
import timeit


def prepare(dataset, size=5):
    # (session, time, word, parity_error, attack)
    sessions = set([row[0] for row in dataset])

    # empty output array
    graph_train = []
    # loop through each word sent on the bus
    for s in sessions:
        words = [row for row in dataset if row[0] == s]
        for i in range(len(words)):
            win = words[max(0, i-size+1):i+1]
            if len(win) == size:
                # unpack integer into bytes
                x = [[v] for row in win for v in list(struct.pack(">I", row[2]))]
                # attack label 
                y = max([row[-1] for row in win])
                
                temp_data = torch_geometric.data.Data(
                    x=torch.LongTensor(x), 
                    edge_index=torch.LongTensor([list(range(0, size-1)), list(range(1, size))]), 
                    y=torch.LongTensor([1 if y != 0 else 0]),
                    ya=torch.LongTensor([y]),
                )
#                 print(temp_data)
                
                graph_train.append(temp_data)
        #         if len(graph_train) >= 10:
        #             break
        # if len(graph_train) >= 10:
        #             break
 
    
    
    dataset = torch_geometric.data.Batch().from_data_list(graph_train)


    return dataset
    

def eval_imp(training_set, testing_sets,  total_attacks=11):
    device = torch.device('cuda')
    print('preparing data')
    dataset = prepare(training_set).to_data_list()
    
    print('all data', dataset[:2], len(dataset))

    train_loader = DataLoader(dataset, batch_size=128)
    
    model = IGNN(nfeat=16,
                nhid=64,
                nclass=total_attacks,
                num_node = None,
                dropout=.5,
                kappa=.9
                ).to(device)
    
    optimizer = torch.optim.Adam(model.parameters(), lr=0.001, weight_decay=0)
    
    def train(epoch):
        model.train()

        if epoch == 51:
            for param_group in optimizer.param_groups:
                param_group['lr'] = 0.5 * param_group['lr']

        loss_all = 0
        for data in tqdm(train_loader):
            # print(data)
            data = data.to(device)
            
            optimizer.zero_grad()
            edge_weight = torch.ones((data.edge_index.size(1), ), dtype=torch.float32, device=data.edge_index.device)
            adj_sp = csr_matrix((edge_weight.cpu().numpy(), (data.edge_index[0,:].cpu().numpy(), data.edge_index[1,:].cpu().numpy() )), shape=(data.num_nodes, data.num_nodes))
            adj_normalizer = fetch_normalization("AugNormAdj")
            adj_sp_nz = adj_normalizer(adj_sp)
            adj = torch.sparse.FloatTensor(torch.LongTensor(np.array([adj_sp_nz.row,adj_sp_nz.col])).to(device), torch.Tensor(adj_sp_nz.data).to(device), torch.Size([data.num_nodes, data.num_nodes])) #normalized adj

            output = model(data.x.T, adj, data.batch)
            loss = F.nll_loss(output, data.y)
            loss.backward()
            loss_all += loss.item() * data.num_graphs
            optimizer.step()
            # break
        return loss_all / len(dataset)


    results = []
    for epoch in range(1, 5+1):
        train_loss = train(epoch)
        # results.append(test_acc)
        print('Epoch: {:03d}, Train Loss: {:.7f}, '.format(epoch, train_loss,))
    
    print('start testing')
    predictions = []
    for test in testing_sets:
        start = timeit.default_timer()
        test_dataset = prepare(test).to_data_list()

        test_loader = DataLoader(test_dataset, batch_size=128)
        
        predict_matrix = []
        ys_anomaly = []
        ys_attack = []
        for data in tqdm(test_loader):
            # print(data)
            data = data.to(device)
            
            optimizer.zero_grad()
            edge_weight = torch.ones((data.edge_index.size(1), ), dtype=torch.float32, device=data.edge_index.device)
            adj_sp = csr_matrix((edge_weight.cpu().numpy(), (data.edge_index[0,:].cpu().numpy(), data.edge_index[1,:].cpu().numpy() )), shape=(data.num_nodes, data.num_nodes))
            adj_normalizer = fetch_normalization("AugNormAdj")
            adj_sp_nz = adj_normalizer(adj_sp)
            adj = torch.sparse.FloatTensor(torch.LongTensor(np.array([adj_sp_nz.row,adj_sp_nz.col])).to(device), torch.Tensor(adj_sp_nz.data).to(device), torch.Size([data.num_nodes, data.num_nodes])) #normalized adj

            output = torch.exp(model(data.x.T, adj, data.batch))
            predict_matrix.extend(output.detach().cpu().numpy().tolist())
            ys_anomaly.extend(data.y.cpu().numpy().tolist())
            ys_attack.extend(data.ya.cpu().numpy().tolist())
            # break
            
        stop = timeit.default_timer()
        predictions.append(((ys_anomaly, ys_attack), predict_matrix, stop - start))
    return model, predictions
    
