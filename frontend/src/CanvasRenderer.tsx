import React, { useEffect, useRef } from 'react';

export interface CanvasPrimitive {
  type: 'Rectangle' | 'Circle' | 'Text' | 'Line';
  x?: number;
  y?: number;
  width?: number;
  height?: number;
  radius?: number;
  x1?: number;
  y1?: number;
  x2?: number;
  y2?: number;
  content?: string;
  size?: number;
  color?: string;
}

interface CanvasRendererProps {
  primitives: CanvasPrimitive[];
}

const CanvasRenderer: React.FC<CanvasRendererProps> = ({ primitives }) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // Clear canvas
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // Draw primitives
    primitives.forEach(p => {
      ctx.fillStyle = p.color || 'white';
      ctx.strokeStyle = p.color || 'white';
      ctx.font = `${p.size || 14}px Inter, sans-serif`;

      switch (p.type) {
        case 'Rectangle':
          ctx.fillRect(p.x || 0, p.y || 0, p.width || 0, p.height || 0);
          break;
        case 'Circle':
          ctx.beginPath();
          ctx.arc(p.x || 0, p.y || 0, p.radius || 0, 0, Math.PI * 2);
          ctx.fill();
          break;
        case 'Line':
          ctx.beginPath();
          ctx.moveTo(p.x1 || 0, p.y1 || 0);
          ctx.lineTo(p.x2 || 0, p.y2 || 0);
          ctx.stroke();
          break;
        case 'Text':
          ctx.fillText(p.content || '', p.x || 0, p.y || 0);
          break;
      }
    });
  }, [primitives]);

  return (
    <div className="canvas-container">
      <canvas
        ref={canvasRef}
        width={800}
        height={600}
        className="main-canvas"
      />
    </div>
  );
};

export default CanvasRenderer;
