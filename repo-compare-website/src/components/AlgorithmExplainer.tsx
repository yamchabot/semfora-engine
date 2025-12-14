import { useState } from 'react';
import { ChevronDown, ChevronRight, Calculator, GitCompare, Network, Shield } from 'lucide-react';

interface AlgorithmSection {
  id: string;
  title: string;
  icon: React.ReactNode;
  description: string;
  formula?: string;
  example?: string;
}

const algorithms: AlgorithmSection[] = [
  {
    id: 'jaccard',
    title: 'Jaccard Similarity (Vocabulary)',
    icon: <Calculator className="w-5 h-5" />,
    description: `
      Measures vocabulary overlap between two repositories by comparing the sets of
      words extracted from symbol names. Higher values indicate more shared terminology.
    `,
    formula: 'J(A,B) = |A ∩ B| / |A ∪ B|',
    example: 'If repo A has words {get, set, user, save} and repo B has {get, set, product, delete}, the intersection is {get, set} (2 words) and union is {get, set, user, save, product, delete} (6 words). Jaccard = 2/6 = 0.33',
  },
  {
    id: 'structural',
    title: 'Structural Similarity',
    icon: <GitCompare className="w-5 h-5" />,
    description: `
      Compares code organization by looking at file counts, symbol density
      (symbols per file), and modularity (modules per file). Uses ratio of smaller
      to larger value for each metric.
    `,
    formula: 'Structural = avg(sizeRatio, densityRatio, modularityRatio)',
    example: 'If repo A has 1000 files and repo B has 4000 files, sizeRatio = 1000/4000 = 0.25. Similar calculations for density and modularity.',
  },
  {
    id: 'complexity',
    title: 'Complexity Similarity',
    icon: <Network className="w-5 h-5" />,
    description: `
      Analyzes call graph topology including average calls per symbol, maximum
      function complexity, edge density, and call concentration (how much
      complexity is concentrated in "god functions").
    `,
    formula: 'Concentration = maxCalls / avgCalls',
    example: 'A concentration of 14x means the most complex function makes 14x more calls than average. Higher concentration suggests more architectural bottlenecks.',
  },
  {
    id: 'risk',
    title: 'Risk Profile Similarity',
    icon: <Shield className="w-5 h-5" />,
    description: `
      Compares the distribution of behavioral risk levels (high/medium/low) using
      Chi-squared distance. Lower distance means more similar risk profiles.
    `,
    formula: 'χ² = Σ[(Oᵢ - Eᵢ)² / (Oᵢ + Eᵢ)]',
    example: 'If repo A is 95% high-risk and repo B is 80% high-risk, the Chi-squared distance captures this difference in risk distribution.',
  },
];

export function AlgorithmExplainer() {
  const [expanded, setExpanded] = useState<string | null>(null);

  return (
    <div className="bg-gray-800 rounded-lg p-4">
      <h3 className="text-lg font-semibold text-white mb-4">
        How Similarity is Calculated
        <span className="ml-2 text-sm font-normal text-gray-400">(No AI Required)</span>
      </h3>

      <div className="space-y-2">
        {algorithms.map(algo => (
          <div key={algo.id} className="border border-gray-700 rounded-lg overflow-hidden">
            <button
              onClick={() => setExpanded(expanded === algo.id ? null : algo.id)}
              className="w-full flex items-center gap-3 p-3 hover:bg-gray-700/50 transition-colors"
            >
              <span className="text-purple-400">{algo.icon}</span>
              <span className="text-white font-medium flex-1 text-left">{algo.title}</span>
              {expanded === algo.id ? (
                <ChevronDown className="w-4 h-4 text-gray-400" />
              ) : (
                <ChevronRight className="w-4 h-4 text-gray-400" />
              )}
            </button>

            {expanded === algo.id && (
              <div className="p-4 pt-0 space-y-3">
                <p className="text-sm text-gray-300 leading-relaxed">
                  {algo.description}
                </p>

                {algo.formula && (
                  <div className="bg-gray-900 rounded p-3">
                    <span className="text-xs text-gray-500">Formula:</span>
                    <code className="block text-sm text-green-400 font-mono mt-1">
                      {algo.formula}
                    </code>
                  </div>
                )}

                {algo.example && (
                  <div className="bg-gray-900/50 rounded p-3 border-l-2 border-purple-500">
                    <span className="text-xs text-gray-500">Example:</span>
                    <p className="text-sm text-gray-400 mt-1">{algo.example}</p>
                  </div>
                )}
              </div>
            )}
          </div>
        ))}
      </div>

      <div className="mt-4 p-3 bg-purple-900/20 rounded border border-purple-500/30">
        <p className="text-sm text-purple-300">
          <strong>Overall Similarity</strong> is a weighted average of all dimensions:
          Vocabulary (20%), Structural (15%), Complexity (20%), Architecture (15%),
          Call Pattern (15%), Risk Profile (15%)
        </p>
      </div>
    </div>
  );
}
