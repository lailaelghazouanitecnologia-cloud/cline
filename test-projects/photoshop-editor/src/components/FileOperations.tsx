import React, { useState } from 'react';
import './file-operations.css';

const FileOperations = () => {
  const [file, setFile] = useState(null);

  const handleOpen = (e: React.MouseEvent) => {
    // open file
  };

  const handleSave = (e: React.MouseEvent) => {
    // save file
  };

  return (
    <div className='file-operations'>
      <button id='open' onClick={handleOpen}>Open</button>
      <button id='save' onClick={handleSave}>Save</button>
    </div>
  );
};

export default FileOperations;
