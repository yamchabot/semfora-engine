import type { RepoOverview } from '../data/repos';
import { GitBranch, Code, Box, AlertTriangle, Copy } from 'lucide-react';

interface RepoCardProps {
  repo: RepoOverview;
  color: 'blue' | 'green';
}

export function RepoCard({ repo, color }: RepoCardProps) {
  const colorClasses = {
    blue: {
      border: 'border-blue-500/30',
      bg: 'bg-blue-500/10',
      text: 'text-blue-400',
      badge: 'bg-blue-900/50 text-blue-300',
    },
    green: {
      border: 'border-green-500/30',
      bg: 'bg-green-500/10',
      text: 'text-green-400',
      badge: 'bg-green-900/50 text-green-300',
    },
  }[color];

  const totalRisk = repo.riskBreakdown.high + repo.riskBreakdown.medium + repo.riskBreakdown.low;
  const highRiskPercent = ((repo.riskBreakdown.high / totalRisk) * 100).toFixed(0);

  return (
    <div className={`rounded-lg border ${colorClasses.border} ${colorClasses.bg} p-4`}>
      <div className="flex items-start justify-between mb-3">
        <div>
          <h3 className={`text-xl font-bold ${colorClasses.text}`}>{repo.name}</h3>
          <p className="text-sm text-gray-400 mt-1">{repo.description}</p>
        </div>
      </div>

      {/* Patterns */}
      <div className="flex flex-wrap gap-1 mb-4">
        {repo.patterns.map(pattern => (
          <span key={pattern} className={`px-2 py-0.5 rounded text-xs ${colorClasses.badge}`}>
            {pattern}
          </span>
        ))}
      </div>

      {/* Stats Grid */}
      <div className="grid grid-cols-2 gap-3">
        <div className="flex items-center gap-2">
          <Code className="w-4 h-4 text-gray-500" />
          <span className="text-sm text-gray-300">
            {repo.stats.files.toLocaleString()} files
          </span>
        </div>
        <div className="flex items-center gap-2">
          <Box className="w-4 h-4 text-gray-500" />
          <span className="text-sm text-gray-300">
            {repo.stats.symbols.toLocaleString()} symbols
          </span>
        </div>
        <div className="flex items-center gap-2">
          <GitBranch className="w-4 h-4 text-gray-500" />
          <span className="text-sm text-gray-300">
            {repo.stats.modules} modules
          </span>
        </div>
        <div className="flex items-center gap-2">
          <AlertTriangle className="w-4 h-4 text-gray-500" />
          <span className="text-sm text-gray-300">
            {highRiskPercent}% high risk
          </span>
        </div>
        <div className="flex items-center gap-2">
          <Copy className="w-4 h-4 text-gray-500" />
          <span className="text-sm text-gray-300">
            {repo.duplicates.totalClusters} duplicate clusters
          </span>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-sm text-gray-400">
            {repo.stats.avgCallsPerSymbol} avg calls/symbol
          </span>
        </div>
      </div>
    </div>
  );
}
