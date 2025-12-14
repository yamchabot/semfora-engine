import {
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  Legend,
  ResponsiveContainer,
  RadarChart,
  PolarGrid,
  PolarAngleAxis,
  PolarRadiusAxis,
  Radar,
} from 'recharts';
import type { RepoOverview } from '../data/repos';
import type { ComparisonResult } from '../lib/comparison';

interface MetricComparisonProps {
  repoA: RepoOverview;
  repoB: RepoOverview;
  comparison: ComparisonResult;
}

export function StructuralComparison({ repoA, repoB }: MetricComparisonProps) {
  const data = [
    { name: 'Files', [repoA.name]: repoA.stats.files, [repoB.name]: repoB.stats.files },
    { name: 'Symbols', [repoA.name]: repoA.stats.symbols, [repoB.name]: repoB.stats.symbols },
    { name: 'Modules', [repoA.name]: repoA.stats.modules, [repoB.name]: repoB.stats.modules },
  ];

  return (
    <div className="bg-gray-800 rounded-lg p-4">
      <h3 className="text-lg font-semibold text-white mb-4">Structural Comparison</h3>
      <ResponsiveContainer width="100%" height={300}>
        <BarChart data={data}>
          <CartesianGrid strokeDasharray="3 3" stroke="#374151" />
          <XAxis dataKey="name" stroke="#9ca3af" />
          <YAxis stroke="#9ca3af" />
          <Tooltip
            contentStyle={{ backgroundColor: '#1f2937', border: 'none', borderRadius: '8px' }}
            labelStyle={{ color: '#fff' }}
          />
          <Legend />
          <Bar dataKey={repoA.name} fill="#3b82f6" />
          <Bar dataKey={repoB.name} fill="#10b981" />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

export function RiskComparison({ repoA, repoB }: MetricComparisonProps) {
  const data = [
    {
      name: 'High Risk',
      [repoA.name]: repoA.riskBreakdown.high,
      [repoB.name]: repoB.riskBreakdown.high,
    },
    {
      name: 'Medium Risk',
      [repoA.name]: repoA.riskBreakdown.medium,
      [repoB.name]: repoB.riskBreakdown.medium,
    },
    {
      name: 'Low Risk',
      [repoA.name]: repoA.riskBreakdown.low,
      [repoB.name]: repoB.riskBreakdown.low,
    },
  ];

  return (
    <div className="bg-gray-800 rounded-lg p-4">
      <h3 className="text-lg font-semibold text-white mb-4">Risk Distribution</h3>
      <ResponsiveContainer width="100%" height={300}>
        <BarChart data={data} layout="vertical">
          <CartesianGrid strokeDasharray="3 3" stroke="#374151" />
          <XAxis type="number" stroke="#9ca3af" />
          <YAxis dataKey="name" type="category" stroke="#9ca3af" width={100} />
          <Tooltip
            contentStyle={{ backgroundColor: '#1f2937', border: 'none', borderRadius: '8px' }}
            labelStyle={{ color: '#fff' }}
          />
          <Legend />
          <Bar dataKey={repoA.name} fill="#ef4444" />
          <Bar dataKey={repoB.name} fill="#f97316" />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

export function SimilarityRadar({ comparison }: { comparison: ComparisonResult }) {
  const data = [
    { dimension: 'Vocabulary', value: comparison.dimensions.vocabulary * 100 },
    { dimension: 'Structure', value: comparison.dimensions.structural * 100 },
    { dimension: 'Complexity', value: comparison.dimensions.complexity * 100 },
    { dimension: 'Architecture', value: comparison.dimensions.architecture * 100 },
    { dimension: 'Call Pattern', value: comparison.dimensions.callPattern * 100 },
    { dimension: 'Risk Profile', value: comparison.dimensions.riskProfile * 100 },
  ];

  return (
    <div className="bg-gray-800 rounded-lg p-4">
      <h3 className="text-lg font-semibold text-white mb-4">Similarity Dimensions</h3>
      <ResponsiveContainer width="100%" height={350}>
        <RadarChart data={data}>
          <PolarGrid stroke="#374151" />
          <PolarAngleAxis dataKey="dimension" stroke="#9ca3af" />
          <PolarRadiusAxis angle={30} domain={[0, 100]} stroke="#9ca3af" />
          <Radar
            name="Similarity %"
            dataKey="value"
            stroke="#8b5cf6"
            fill="#8b5cf6"
            fillOpacity={0.5}
          />
          <Tooltip
            contentStyle={{ backgroundColor: '#1f2937', border: 'none', borderRadius: '8px' }}
            formatter={(value: number) => [`${value.toFixed(1)}%`, 'Similarity']}
          />
        </RadarChart>
      </ResponsiveContainer>
    </div>
  );
}

export function ComplexityComparison({ repoA, repoB, comparison }: MetricComparisonProps) {
  const data = [
    {
      name: 'Avg Calls/Symbol',
      [repoA.name]: repoA.stats.avgCallsPerSymbol,
      [repoB.name]: repoB.stats.avgCallsPerSymbol,
    },
    {
      name: 'Edge Density',
      [repoA.name]: comparison.complexityComparison.edgeDensity.a,
      [repoB.name]: comparison.complexityComparison.edgeDensity.b,
    },
  ];

  return (
    <div className="bg-gray-800 rounded-lg p-4">
      <h3 className="text-lg font-semibold text-white mb-4">Complexity Metrics</h3>
      <ResponsiveContainer width="100%" height={250}>
        <BarChart data={data}>
          <CartesianGrid strokeDasharray="3 3" stroke="#374151" />
          <XAxis dataKey="name" stroke="#9ca3af" />
          <YAxis stroke="#9ca3af" />
          <Tooltip
            contentStyle={{ backgroundColor: '#1f2937', border: 'none', borderRadius: '8px' }}
            labelStyle={{ color: '#fff' }}
            formatter={(value: number) => value.toFixed(2)}
          />
          <Legend />
          <Bar dataKey={repoA.name} fill="#06b6d4" />
          <Bar dataKey={repoB.name} fill="#ec4899" />
        </BarChart>
      </ResponsiveContainer>

      <div className="mt-4 grid grid-cols-2 gap-4 text-sm">
        <div className="bg-gray-700 rounded p-3">
          <span className="text-gray-400">Max Calls in Symbol</span>
          <div className="flex justify-between mt-1">
            <span className="text-cyan-400">{repoA.name}: {repoA.stats.maxCallsInSymbol}</span>
            <span className="text-pink-400">{repoB.name}: {repoB.stats.maxCallsInSymbol}</span>
          </div>
        </div>
        <div className="bg-gray-700 rounded p-3">
          <span className="text-gray-400">Call Concentration</span>
          <div className="flex justify-between mt-1">
            <span className="text-cyan-400">{comparison.complexityComparison.callConcentration.a.toFixed(1)}x</span>
            <span className="text-pink-400">{comparison.complexityComparison.callConcentration.b.toFixed(1)}x</span>
          </div>
        </div>
      </div>
    </div>
  );
}
