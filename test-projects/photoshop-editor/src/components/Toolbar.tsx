import React, { useState } from 'react';
import './toolbar.css';

const Toolbar = () => {
  const [tool, setTool] = useState('brush');

  const handleToolChange = (e: React.MouseEvent) => {
    setTool(e.currentTarget.id);
  };

  return (
    <div className='toolbar'>
      <button id='brush' onClick={handleToolChange}>Brush</button>
      <button id='eraser' onClick={handleToolChange}>Eraser</button>
      <button id='selection' onClick={handleToolChange}>Selection</button>
      <button id='move' onClick={handleToolChange}>Move</button>
    </div>
  );
};

export default Toolbar;
