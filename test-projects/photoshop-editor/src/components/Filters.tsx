import React from 'react';
import './filters.css';

const Filters = () => {
  const handleFilterChange = (e: React.MouseEvent) => {
    // apply filter
  };

  return (
    <div className='filters'>
      <button id='blur' onClick={handleFilterChange}>Blur</button>
      <button id='brightness' onClick={handleFilterChange}>Brightness</button>
      <button id='contrast' onClick={handleFilterChange}>Contrast</button>
    </div>
  );
};

export default Filters;
