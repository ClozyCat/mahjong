import { useEffect, useRef, useState, useCallback } from 'react';

interface Point {
  x: number;
  y: number;
}

interface SnakeOverlayProps {
  onGameOver?: () => void;
}

const GRID_SIZE = 24;
const INITIAL_SPEED = 120;

export function SnakeOverlay({ onGameOver }: SnakeOverlayProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [snake, setSnake] = useState<Point[]>([{ x: 5, y: 5 }, { x: 4, y: 5 }, { x: 3, y: 5 }]);
  const [food, setFood] = useState<Point>({ x: 10, y: 10 });
  const [direction, setDirection] = useState<Point>({ x: 1, y: 0 });
  const [nextDirection, setNextDirection] = useState<Point>({ x: 1, y: 0 });
  const [isGameOver, setIsGameOver] = useState(false);
  const [isPaused, setIsPaused] = useState(false);
  
  const moveSnake = useCallback(() => {
    if (isGameOver || isPaused) return;

    setSnake((prevSnake) => {
      const head = prevSnake[0];
      const newHead = { x: head.x + nextDirection.x, y: head.y + nextDirection.y };
      setDirection(nextDirection);

      // Check collision with edges
      if (containerRef.current) {
        const { width, height } = containerRef.current.getBoundingClientRect();
        const cols = Math.floor(width / GRID_SIZE);
        const rows = Math.floor(height / GRID_SIZE);

        if (newHead.x < 0 || newHead.x >= cols || newHead.y < 0 || newHead.y >= rows) {
          setIsGameOver(true);
          onGameOver?.();
          return prevSnake;
        }
      }

      // Check collision with self
      if (prevSnake.some((segment) => segment.x === newHead.x && segment.y === newHead.y)) {
        setIsGameOver(true);
        onGameOver?.();
        return prevSnake;
      }

      const newSnake = [newHead, ...prevSnake];

      // Check food
      if (newHead.x === food.x && newHead.y === food.y) {
        generateFood(newSnake);
      } else {
        newSnake.pop();
      }

      return newSnake;
    });
  }, [nextDirection, food, isGameOver, isPaused, onGameOver]);

  const generateFood = (currentSnake: Point[]) => {
    const container = containerRef.current;
    if (!container) return;
    const { width, height } = container.getBoundingClientRect();
    const cols = Math.floor(width / GRID_SIZE);
    const rows = Math.floor(height / GRID_SIZE);

    let newFood: Point;
    do {
      newFood = {
        x: Math.floor(Math.random() * cols),
        y: Math.floor(Math.random() * rows),
      };
    } while (currentSnake.some((segment) => segment.x === newFood.x && segment.y === newFood.y));
    setFood(newFood);
  };

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const key = e.key.toLowerCase();
      if (['w', 'a', 's', 'd'].includes(key)) {
        setIsPaused(false);
      }

      if (key === 'w' && direction.y === 0) setNextDirection({ x: 0, y: -1 });
      else if (key === 's' && direction.y === 0) setNextDirection({ x: 0, y: 1 });
      else if (key === 'a' && direction.x === 0) setNextDirection({ x: -1, y: 0 });
      else if (key === 'd' && direction.x === 0) setNextDirection({ x: 1, y: 0 });
    };

    const handlePointerDown = () => {
      setIsPaused(true);
    };

    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('pointerdown', handlePointerDown);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('pointerdown', handlePointerDown);
    };
  }, [direction]);

  useEffect(() => {
    const timer = setInterval(moveSnake, INITIAL_SPEED);
    return () => clearInterval(timer);
  }, [moveSnake]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !containerRef.current) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const { width, height } = containerRef.current.getBoundingClientRect();
    canvas.width = width;
    canvas.height = height;

    const draw = () => {
      ctx.clearRect(0, 0, width, height);

      // Get colors from CSS variables
      const style = getComputedStyle(document.documentElement);
      const snakeColor = style.getPropertyValue('--accent').trim() || '#8ea9a1';
      const foodColor = style.getPropertyValue('--accent-2').trim() || '#cfad72';
      const foodGlow = style.getPropertyValue('--theme-warning').trim() || '#d2af72';

      // Draw food
      ctx.fillStyle = foodColor;
      ctx.shadowBlur = 15;
      ctx.shadowColor = foodGlow;
      ctx.beginPath();
      ctx.roundRect(food.x * GRID_SIZE + 2, food.y * GRID_SIZE + 2, GRID_SIZE - 4, GRID_SIZE - 4, 4);
      ctx.fill();
      ctx.shadowBlur = 0;

      // Draw snake
      snake.forEach((segment, index) => {
        const baseOpacity = 100 - index * 2;
        ctx.fillStyle = index === 0 
          ? snakeColor
          : `color-mix(in srgb, ${snakeColor} ${baseOpacity}%, transparent)`;
        
        ctx.beginPath();
        const padding = index === 0 ? 1 : 2;
        ctx.roundRect(
          segment.x * GRID_SIZE + padding,
          segment.y * GRID_SIZE + padding,
          GRID_SIZE - padding * 2,
          GRID_SIZE - padding * 2,
          index === 0 ? 6 : 4
        );
        ctx.fill();

        // Head eye
        if (index === 0) {
          ctx.fillStyle = 'rgba(0,0,0,0.3)';
          const eyeSize = 3;
          let eyeX = segment.x * GRID_SIZE + GRID_SIZE / 2;
          let eyeY = segment.y * GRID_SIZE + GRID_SIZE / 2;
          
          if (direction.x === 1) eyeX += 4;
          else if (direction.x === -1) eyeX -= 4;
          else if (direction.y === 1) eyeY += 4;
          else if (direction.y === -1) eyeY -= 4;

          ctx.beginPath();
          ctx.arc(eyeX, eyeY, eyeSize, 0, Math.PI * 2);
          ctx.fill();
        }
      });

      if (isGameOver) {
        ctx.fillStyle = 'rgba(255, 0, 0, 0.2)';
        ctx.fillRect(0, 0, width, height);
        ctx.fillStyle = '#ff4d4f';
        ctx.font = 'bold 32px sans-serif';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText('Game Over', width / 2, height / 2);
      }
    };

    const animId = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(animId);
  }, [snake, food, isGameOver, isPaused, direction]);

    const animId = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(animId);
  }, [snake, food, isGameOver, direction]);

  return (
    <div ref={containerRef} className="snake-overlay">
      <canvas ref={canvasRef} />
    </div>
  );
}
