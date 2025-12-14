import { useMemo } from 'react';

interface SimilarityGaugeProps {
  value: number; // 0-1
  label: string;
  size?: 'sm' | 'md' | 'lg';
}

export function SimilarityGauge({ value, label, size = 'md' }: SimilarityGaugeProps) {
  const percentage = Math.round(value * 100);

  const dimensions = useMemo(() => {
    switch (size) {
      case 'sm': return { width: 80, height: 80, stroke: 6 };
      case 'lg': return { width: 160, height: 160, stroke: 12 };
      default: return { width: 120, height: 120, stroke: 8 };
    }
  }, [size]);

  const radius = (dimensions.width - dimensions.stroke) / 2;
  const circumference = 2 * Math.PI * radius;
  const offset = circumference - (value * circumference);

  const getColor = (val: number) => {
    if (val >= 0.7) return '#22c55e'; // green
    if (val >= 0.4) return '#eab308'; // yellow
    return '#ef4444'; // red
  };

  return (
    <div className="flex flex-col items-center">
      <svg width={dimensions.width} height={dimensions.width} className="transform -rotate-90">
        {/* Background circle */}
        <circle
          cx={dimensions.width / 2}
          cy={dimensions.width / 2}
          r={radius}
          fill="none"
          stroke="#1f2937"
          strokeWidth={dimensions.stroke}
        />
        {/* Progress circle */}
        <circle
          cx={dimensions.width / 2}
          cy={dimensions.width / 2}
          r={radius}
          fill="none"
          stroke={getColor(value)}
          strokeWidth={dimensions.stroke}
          strokeLinecap="round"
          strokeDasharray={circumference}
          strokeDashoffset={offset}
          style={{ transition: 'stroke-dashoffset 0.5s ease' }}
        />
      </svg>
      <div
        className="absolute flex flex-col items-center justify-center"
        style={{
          width: dimensions.width,
          height: dimensions.width,
          marginTop: 0
        }}
      >
        <span className="text-2xl font-bold text-white">{percentage}%</span>
      </div>
      <span className="mt-2 text-sm text-gray-400">{label}</span>
    </div>
  );
}
