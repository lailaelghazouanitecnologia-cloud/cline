import React, { useState, useEffect } from 'react';
import './canvas.css';

interface CanvasProps {
  width: number;
  height: number;
}

const Canvas: React.FC<CanvasProps> = ({ width, height }) => {
  const [ctx, setCtx] = useState<CanvasRenderingContext2D | null>(null);
  const [layers, setLayers] = useState([]);

  useEffect(() => {
    const canvas = document.getElementById('canvas') as HTMLCanvasElement;
    const ctx = canvas.getContext('2d') as CanvasRenderingContext2D;
    setCtx(ctx);
  }, []);

  const handleMouseDown = (e: React.MouseEvent) => {
    // brush tool
    if (e.button === 0) {
      const x = e.clientX;
      const y = e.clientY;
      ctx?.beginPath();
      ctx?.moveTo(x, y);
      ctx?.lineTo(x, y);
      ctx?.stroke();
    }
  };

  return (
    <canvas
      id='canvas'
      width={width}
      height={height}
      onMouseDown={handleMouseDown}
    ></canvas>
  );
};

export default Canvas;
