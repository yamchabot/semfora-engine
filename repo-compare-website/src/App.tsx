import { useMemo } from 'react';
import { daggerfallUnity, nopCommerce } from './data/repos';
import { compareRepositories, explainComparison } from './lib/comparison';
import { SimilarityGauge } from './components/SimilarityGauge';
import { RepoCard } from './components/RepoCard';
import {
  StructuralComparison,
  RiskComparison,
  SimilarityRadar,
  ComplexityComparison,
} from './components/MetricComparison';
import { VocabularyAnalysis, TopCalleesComparison } from './components/VocabularyAnalysis';
import { AlgorithmExplainer } from './components/AlgorithmExplainer';
import { GitCompare, Zap, Brain, Info } from 'lucide-react';

function App() {
  const comparison = useMemo(
    () => compareRepositories(daggerfallUnity, nopCommerce),
    []
  );

  const insights = useMemo(
    () => explainComparison(comparison, daggerfallUnity, nopCommerce),
    [comparison]
  );

  return (
    <div className="min-h-screen bg-gray-900 text-white">
      {/* Header */}
      <header className="bg-gray-800 border-b border-gray-700">
        <div className="max-w-7xl mx-auto px-4 py-6">
          <div className="flex items-center gap-3">
            <GitCompare className="w-8 h-8 text-purple-400" />
            <div>
              <h1 className="text-2xl font-bold">Semfora Repository Comparator</h1>
              <p className="text-sm text-gray-400">
                Comparing C# repositories using semantic analysis - no AI required
              </p>
            </div>
          </div>
        </div>
      </header>

      <main className="max-w-7xl mx-auto px-4 py-8 space-y-8">
        {/* Hero Section - Overall Similarity */}
        <section className="bg-gradient-to-r from-purple-900/30 to-blue-900/30 rounded-xl p-8 border border-purple-500/30">
          <div className="flex flex-col md:flex-row items-center justify-between gap-8">
            <div className="flex-1">
              <div className="flex items-center gap-2 mb-4">
                <Zap className="w-5 h-5 text-yellow-400" />
                <span className="text-sm text-gray-400">Powered by Semfora Semantic Engine</span>
              </div>
              <h2 className="text-3xl font-bold mb-4">
                Comparing{' '}
                <span className="text-blue-400">{daggerfallUnity.name}</span>
                {' '}vs{' '}
                <span className="text-green-400">{nopCommerce.name}</span>
              </h2>
              <p className="text-gray-300 mb-6">
                Two completely different C# applications: a game engine recreation vs an e-commerce platform.
                How similar are they at a structural and semantic level?
              </p>

              {/* Key Insights */}
              <div className="space-y-2">
                {insights.map((insight, i) => (
                  <div key={i} className="flex items-start gap-2">
                    <Brain className="w-4 h-4 text-purple-400 mt-0.5 flex-shrink-0" />
                    <span className="text-sm text-gray-300">{insight}</span>
                  </div>
                ))}
              </div>
            </div>

            <div className="flex flex-col items-center relative">
              <SimilarityGauge
                value={comparison.overallSimilarity}
                label="Overall Similarity"
                size="lg"
              />
            </div>
          </div>
        </section>

        {/* Repository Cards */}
        <section className="grid md:grid-cols-2 gap-6">
          <RepoCard repo={daggerfallUnity} color="blue" />
          <RepoCard repo={nopCommerce} color="green" />
        </section>

        {/* Dimension Gauges */}
        <section className="bg-gray-800 rounded-lg p-6">
          <h2 className="text-xl font-semibold mb-6">Similarity by Dimension</h2>
          <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-4">
            <div className="flex flex-col items-center relative">
              <SimilarityGauge
                value={comparison.dimensions.vocabulary}
                label="Vocabulary"
                size="sm"
              />
            </div>
            <div className="flex flex-col items-center relative">
              <SimilarityGauge
                value={comparison.dimensions.structural}
                label="Structure"
                size="sm"
              />
            </div>
            <div className="flex flex-col items-center relative">
              <SimilarityGauge
                value={comparison.dimensions.complexity}
                label="Complexity"
                size="sm"
              />
            </div>
            <div className="flex flex-col items-center relative">
              <SimilarityGauge
                value={comparison.dimensions.architecture}
                label="Architecture"
                size="sm"
              />
            </div>
            <div className="flex flex-col items-center relative">
              <SimilarityGauge
                value={comparison.dimensions.callPattern}
                label="Call Pattern"
                size="sm"
              />
            </div>
            <div className="flex flex-col items-center relative">
              <SimilarityGauge
                value={comparison.dimensions.riskProfile}
                label="Risk Profile"
                size="sm"
              />
            </div>
          </div>
        </section>

        {/* Charts Section */}
        <section className="grid md:grid-cols-2 gap-6">
          <SimilarityRadar comparison={comparison} />
          <StructuralComparison
            repoA={daggerfallUnity}
            repoB={nopCommerce}
            comparison={comparison}
          />
        </section>

        <section className="grid md:grid-cols-2 gap-6">
          <RiskComparison
            repoA={daggerfallUnity}
            repoB={nopCommerce}
            comparison={comparison}
          />
          <ComplexityComparison
            repoA={daggerfallUnity}
            repoB={nopCommerce}
            comparison={comparison}
          />
        </section>

        {/* Vocabulary Analysis */}
        <VocabularyAnalysis
          repoA={daggerfallUnity}
          repoB={nopCommerce}
          comparison={comparison}
        />

        {/* Top Callees */}
        <TopCalleesComparison repoA={daggerfallUnity} repoB={nopCommerce} />

        {/* Algorithm Explainer */}
        <AlgorithmExplainer />

        {/* Info Banner */}
        <section className="bg-gray-800 rounded-lg p-6 border border-gray-700">
          <div className="flex items-start gap-4">
            <Info className="w-6 h-6 text-blue-400 flex-shrink-0" />
            <div>
              <h3 className="text-lg font-semibold mb-2">How This Works</h3>
              <p className="text-gray-300 text-sm leading-relaxed">
                This comparison uses data extracted by the{' '}
                <strong className="text-purple-400">semfora-engine</strong> semantic analysis tool.
                The engine parses source code, builds call graphs, detects patterns, and computes
                behavioral risk levels. All comparison algorithms are purely mathematical - no AI
                or machine learning is used. The similarity scores are computed using Jaccard indices,
                cosine similarity, Chi-squared distances, and ratio comparisons.
              </p>
              <p className="text-gray-400 text-sm mt-4">
                <strong>Data source:</strong> Generated from{' '}
                <code className="text-green-400">mcp__semfora-engine__get_repo_overview</code>,{' '}
                <code className="text-green-400">mcp__semfora-engine__get_call_graph</code>, and{' '}
                <code className="text-green-400">mcp__semfora-engine__find_duplicates</code> tool outputs.
              </p>
            </div>
          </div>
        </section>
      </main>

      {/* Footer */}
      <footer className="bg-gray-800 border-t border-gray-700 mt-12">
        <div className="max-w-7xl mx-auto px-4 py-6">
          <p className="text-center text-gray-400 text-sm">
            Built with Semfora Engine • React • Recharts • Tailwind CSS
          </p>
        </div>
      </footer>
    </div>
  );
}

export default App;
