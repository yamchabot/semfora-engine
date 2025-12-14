import type { ComparisonResult } from '../lib/comparison';
import type { RepoOverview } from '../data/repos';

interface VocabularyAnalysisProps {
  repoA: RepoOverview;
  repoB: RepoOverview;
  comparison: ComparisonResult;
}

export function VocabularyAnalysis({ repoA, repoB, comparison }: VocabularyAnalysisProps) {
  const { vocabularyOverlap } = comparison;

  return (
    <div className="bg-gray-800 rounded-lg p-4">
      <h3 className="text-lg font-semibold text-white mb-4">
        Vocabulary Analysis
        <span className="ml-2 text-sm font-normal text-gray-400">
          (Jaccard Index: {(vocabularyOverlap.jaccardIndex * 100).toFixed(1)}%)
        </span>
      </h3>

      <div className="grid grid-cols-3 gap-4">
        {/* Shared vocabulary */}
        <div>
          <h4 className="text-sm font-medium text-purple-400 mb-2">
            Shared Terms ({vocabularyOverlap.shared.length})
          </h4>
          <div className="flex flex-wrap gap-1">
            {vocabularyOverlap.shared.map(term => (
              <span
                key={term}
                className="px-2 py-1 bg-purple-900/50 text-purple-300 rounded text-xs"
              >
                {term}
              </span>
            ))}
          </div>
        </div>

        {/* Only in A */}
        <div>
          <h4 className="text-sm font-medium text-blue-400 mb-2">
            Only in {repoA.name} ({vocabularyOverlap.onlyA.length})
          </h4>
          <div className="flex flex-wrap gap-1">
            {vocabularyOverlap.onlyA.map(term => (
              <span
                key={term}
                className="px-2 py-1 bg-blue-900/50 text-blue-300 rounded text-xs"
              >
                {term}
              </span>
            ))}
          </div>
        </div>

        {/* Only in B */}
        <div>
          <h4 className="text-sm font-medium text-green-400 mb-2">
            Only in {repoB.name} ({vocabularyOverlap.onlyB.length})
          </h4>
          <div className="flex flex-wrap gap-1">
            {vocabularyOverlap.onlyB.map(term => (
              <span
                key={term}
                className="px-2 py-1 bg-green-900/50 text-green-300 rounded text-xs"
              >
                {term}
              </span>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

export function TopCalleesComparison({ repoA, repoB }: { repoA: RepoOverview; repoB: RepoOverview }) {
  return (
    <div className="bg-gray-800 rounded-lg p-4">
      <h3 className="text-lg font-semibold text-white mb-4">Top Called Functions</h3>

      <div className="grid grid-cols-2 gap-6">
        {/* Repo A */}
        <div>
          <h4 className="text-sm font-medium text-blue-400 mb-3">{repoA.name}</h4>
          <div className="space-y-2">
            {repoA.topCallees.slice(0, 8).map((callee, i) => (
              <div key={i} className="flex items-center justify-between">
                <span className="text-xs text-gray-300 font-mono truncate max-w-[200px]">
                  {callee.name.split('.').pop()}
                </span>
                <div className="flex items-center gap-2">
                  <div
                    className="h-2 bg-blue-500 rounded"
                    style={{ width: `${Math.min(100, callee.count / 4)}px` }}
                  />
                  <span className="text-xs text-gray-500 w-10 text-right">{callee.count}</span>
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Repo B */}
        <div>
          <h4 className="text-sm font-medium text-green-400 mb-3">{repoB.name}</h4>
          <div className="space-y-2">
            {repoB.topCallees.slice(0, 8).map((callee, i) => (
              <div key={i} className="flex items-center justify-between">
                <span className="text-xs text-gray-300 font-mono truncate max-w-[200px]">
                  {callee.name.split('.').pop()}
                </span>
                <div className="flex items-center gap-2">
                  <div
                    className="h-2 bg-green-500 rounded"
                    style={{ width: `${Math.min(100, callee.count / 9)}px` }}
                  />
                  <span className="text-xs text-gray-500 w-10 text-right">{callee.count}</span>
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className="mt-4 p-3 bg-gray-700 rounded">
        <h4 className="text-sm font-medium text-gray-300 mb-2">Pattern Analysis</h4>
        <p className="text-xs text-gray-400">
          Both repos heavily use <code className="text-purple-400">string.Format</code> and{' '}
          <code className="text-purple-400">string.IsNullOrEmpty</code>, indicating similar
          string manipulation patterns. However:
        </p>
        <ul className="mt-2 text-xs text-gray-400 list-disc list-inside space-y-1">
          <li>
            <span className="text-blue-400">{repoA.name}</span> relies on Unity-specific APIs
            (TextManager, DaggerfallUI, Vector2)
          </li>
          <li>
            <span className="text-green-400">{repoB.name}</span> uses ASP.NET patterns
            (async services, dependency injection, RedirectToAction)
          </li>
        </ul>
      </div>
    </div>
  );
}
