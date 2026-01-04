import React from 'react';
import ReactDOM from 'react-dom';
import './index.css';
import App from './App';
import Toolbar from './components/Toolbar';
import Filters from './components/Filters';
import FileOperations from './components/FileOperations';

ReactDOM.render(
  <React.StrictMode>
    <App />
    <Toolbar />
    <Filters />
    <FileOperations />
  </React.StrictMode>,
  document.getElementById('root')
);
