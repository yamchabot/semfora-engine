/**
 * Repository Comparison Algorithms
 *
 * These algorithms compare repositories WITHOUT using AI.
 * Instead, they use mathematical and statistical techniques to quantify similarity.
 */

import type { RepoOverview } from "../data/repos";

// ============================================================================
// CORE SIMILARITY ALGORITHMS
// ============================================================================

/**
 * Jaccard Similarity: Measures vocabulary overlap between two sets
 * J(A,B) = |A ∩ B| / |A ∪ B|
 * Range: 0 (no overlap) to 1 (identical)
 */
export function jaccardSimilarity(setA: Set<string>, setB: Set<string>): number {
  const intersection = new Set([...setA].filter(x => setB.has(x)));
  const union = new Set([...setA, ...setB]);

  if (union.size === 0) return 0;
  return intersection.size / union.size;
}

/**
 * Cosine Similarity: Measures angle between two vectors
 * cos(θ) = (A·B) / (|A| × |B|)
 * Range: 0 to 1 (for positive vectors)
 */
export function cosineSimilarity(vectorA: number[], vectorB: number[]): number {
  if (vectorA.length !== vectorB.length) {
    throw new Error("Vectors must have same length");
  }

  let dotProduct = 0;
  let magnitudeA = 0;
  let magnitudeB = 0;

  for (let i = 0; i < vectorA.length; i++) {
    dotProduct += vectorA[i] * vectorB[i];
    magnitudeA += vectorA[i] * vectorA[i];
    magnitudeB += vectorB[i] * vectorB[i];
  }

  magnitudeA = Math.sqrt(magnitudeA);
  magnitudeB = Math.sqrt(magnitudeB);

  if (magnitudeA === 0 || magnitudeB === 0) return 0;
  return dotProduct / (magnitudeA * magnitudeB);
}

/**
 * Normalize a value to 0-1 range using min-max scaling
 */
export function normalize(value: number, min: number, max: number): number {
  if (max === min) return 0.5;
  return (value - min) / (max - min);
}

/**
 * Chi-squared distance for comparing distributions
 * Lower values = more similar
 */
export function chiSquaredDistance(histA: number[], histB: number[]): number {
  let distance = 0;
  for (let i = 0; i < histA.length; i++) {
    const sum = histA[i] + histB[i];
    if (sum > 0) {
      distance += Math.pow(histA[i] - histB[i], 2) / sum;
    }
  }
  return distance / 2;
}

// ============================================================================
// TOKENIZATION & VOCABULARY ANALYSIS
// ============================================================================

/**
 * Tokenize a symbol name into component words
 * Handles: camelCase, PascalCase, snake_case, SCREAMING_SNAKE
 */
export function tokenizeSymbolName(name: string): string[] {
  // Split on underscores and dots
  let tokens = name.split(/[_.\-]/);

  // Further split on camelCase boundaries
  tokens = tokens.flatMap(token => {
    // Split on lowercase->uppercase transitions
    return token.split(/(?<=[a-z])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])/);
  });

  // Normalize to lowercase and filter empty
  return tokens
    .map(t => t.toLowerCase())
    .filter(t => t.length > 1); // Ignore single chars
}

/**
 * Extract vocabulary from a repository's symbols
 */
export function extractVocabulary(repo: RepoOverview): Set<string> {
  const vocabulary = new Set<string>();

  // From sample symbols
  repo.sampleSymbols.forEach(sym => {
    tokenizeSymbolName(sym.name).forEach(t => vocabulary.add(t));
  });

  // From top callees
  repo.topCallees.forEach(callee => {
    tokenizeSymbolName(callee.name).forEach(t => vocabulary.add(t));
  });

  // From module names
  repo.moduleStats.forEach(mod => {
    tokenizeSymbolName(mod.name).forEach(t => vocabulary.add(t));
  });

  return vocabulary;
}

// ============================================================================
// REPOSITORY COMPARISON METRICS
// ============================================================================

export interface ComparisonResult {
  // Overall similarity score (0-1)
  overallSimilarity: number;

  // Individual dimension scores
  dimensions: {
    vocabulary: number;      // Symbol naming similarity
    structural: number;      // Code structure similarity
    complexity: number;      // Complexity profile similarity
    architecture: number;    // Module organization similarity
    callPattern: number;     // Call graph topology similarity
    riskProfile: number;     // Risk distribution similarity
  };

  // Detailed breakdowns
  vocabularyOverlap: {
    shared: string[];
    onlyA: string[];
    onlyB: string[];
    jaccardIndex: number;
  };

  structuralComparison: {
    sizeRatio: number;
    symbolDensity: { a: number; b: number };
    modularity: { a: number; b: number };
  };

  complexityComparison: {
    avgCallsRatio: number;
    maxCallsRatio: number;
    edgeDensity: { a: number; b: number };
    callConcentration: { a: number; b: number };
  };

  riskComparison: {
    distributions: {
      a: { high: number; medium: number; low: number };
      b: { high: number; medium: number; low: number };
    };
    chiSquaredDistance: number;
  };
}

/**
 * Compare two repositories and compute similarity metrics
 */
export function compareRepositories(
  repoA: RepoOverview,
  repoB: RepoOverview
): ComparisonResult {
  // 1. Vocabulary Similarity
  const vocabA = extractVocabulary(repoA);
  const vocabB = extractVocabulary(repoB);
  const sharedVocab = [...vocabA].filter(t => vocabB.has(t));
  const onlyA = [...vocabA].filter(t => !vocabB.has(t));
  const onlyB = [...vocabB].filter(t => !vocabA.has(t));
  const vocabularySimilarity = jaccardSimilarity(vocabA, vocabB);

  // 2. Structural Similarity
  const sizeRatio = Math.min(repoA.stats.files, repoB.stats.files) /
                    Math.max(repoA.stats.files, repoB.stats.files);
  const symbolDensityA = repoA.stats.symbols / repoA.stats.files;
  const symbolDensityB = repoB.stats.symbols / repoB.stats.files;
  const densityRatio = Math.min(symbolDensityA, symbolDensityB) /
                       Math.max(symbolDensityA, symbolDensityB);
  const modularityA = repoA.stats.modules / repoA.stats.files;
  const modularityB = repoB.stats.modules / repoB.stats.files;
  const modularityRatio = Math.min(modularityA, modularityB) /
                          Math.max(modularityA, modularityB);
  const structuralSimilarity = (sizeRatio + densityRatio + modularityRatio) / 3;

  // 3. Complexity Similarity
  const avgCallsRatio = Math.min(repoA.stats.avgCallsPerSymbol, repoB.stats.avgCallsPerSymbol) /
                        Math.max(repoA.stats.avgCallsPerSymbol, repoB.stats.avgCallsPerSymbol);
  const maxCallsRatio = Math.min(repoA.stats.maxCallsInSymbol, repoB.stats.maxCallsInSymbol) /
                        Math.max(repoA.stats.maxCallsInSymbol, repoB.stats.maxCallsInSymbol);
  const edgeDensityA = repoA.stats.callEdges / repoA.stats.symbols;
  const edgeDensityB = repoB.stats.callEdges / repoB.stats.symbols;
  const edgeRatio = Math.min(edgeDensityA, edgeDensityB) /
                    Math.max(edgeDensityA, edgeDensityB);
  // Call concentration: how much is concentrated in top callers
  const concentrationA = repoA.stats.maxCallsInSymbol / repoA.stats.avgCallsPerSymbol;
  const concentrationB = repoB.stats.maxCallsInSymbol / repoB.stats.avgCallsPerSymbol;
  const concentrationRatio = Math.min(concentrationA, concentrationB) /
                             Math.max(concentrationA, concentrationB);
  const complexitySimilarity = (avgCallsRatio + maxCallsRatio * 0.5 + edgeRatio + concentrationRatio * 0.5) / 3;

  // 4. Architecture Similarity (module organization)
  const moduleVocabA = new Set(repoA.moduleStats.map(m => m.purpose.toLowerCase()));
  const moduleVocabB = new Set(repoB.moduleStats.map(m => m.purpose.toLowerCase()));
  const architectureSimilarity = jaccardSimilarity(moduleVocabA, moduleVocabB);

  // 5. Call Pattern Similarity
  // Compare top callee patterns - are they calling similar types of things?
  const calleeTypesA = new Set(
    repoA.topCallees.map(c => {
      // Categorize callees by type
      const name = c.name.toLowerCase();
      if (name.includes("string") || name.includes("format")) return "string-ops";
      if (name.includes("get") || name.includes("set")) return "accessor";
      if (name.includes("async") || name.includes("await")) return "async";
      if (name.includes("log") || name.includes("debug")) return "logging";
      if (name.includes("new ")) return "constructor";
      return "other";
    })
  );
  const calleeTypesB = new Set(
    repoB.topCallees.map(c => {
      const name = c.name.toLowerCase();
      if (name.includes("string") || name.includes("format")) return "string-ops";
      if (name.includes("get") || name.includes("set")) return "accessor";
      if (name.includes("async") || name.includes("await")) return "async";
      if (name.includes("log") || name.includes("debug")) return "logging";
      if (name.includes("new ")) return "constructor";
      return "other";
    })
  );
  const callPatternSimilarity = jaccardSimilarity(calleeTypesA, calleeTypesB);

  // 6. Risk Profile Similarity
  const totalA = repoA.riskBreakdown.high + repoA.riskBreakdown.medium + repoA.riskBreakdown.low;
  const totalB = repoB.riskBreakdown.high + repoB.riskBreakdown.medium + repoB.riskBreakdown.low;
  const riskDistA = [
    repoA.riskBreakdown.high / totalA,
    repoA.riskBreakdown.medium / totalA,
    repoA.riskBreakdown.low / totalA,
  ];
  const riskDistB = [
    repoB.riskBreakdown.high / totalB,
    repoB.riskBreakdown.medium / totalB,
    repoB.riskBreakdown.low / totalB,
  ];
  const riskChiSquared = chiSquaredDistance(riskDistA, riskDistB);
  // Convert to similarity (lower chi-squared = more similar)
  const riskProfileSimilarity = 1 / (1 + riskChiSquared * 10);

  // 7. Overall Similarity (weighted average)
  const weights = {
    vocabulary: 0.20,
    structural: 0.15,
    complexity: 0.20,
    architecture: 0.15,
    callPattern: 0.15,
    riskProfile: 0.15,
  };

  const overallSimilarity =
    vocabularySimilarity * weights.vocabulary +
    structuralSimilarity * weights.structural +
    complexitySimilarity * weights.complexity +
    architectureSimilarity * weights.architecture +
    callPatternSimilarity * weights.callPattern +
    riskProfileSimilarity * weights.riskProfile;

  return {
    overallSimilarity,
    dimensions: {
      vocabulary: vocabularySimilarity,
      structural: structuralSimilarity,
      complexity: complexitySimilarity,
      architecture: architectureSimilarity,
      callPattern: callPatternSimilarity,
      riskProfile: riskProfileSimilarity,
    },
    vocabularyOverlap: {
      shared: sharedVocab.slice(0, 20),
      onlyA: onlyA.slice(0, 20),
      onlyB: onlyB.slice(0, 20),
      jaccardIndex: vocabularySimilarity,
    },
    structuralComparison: {
      sizeRatio,
      symbolDensity: { a: symbolDensityA, b: symbolDensityB },
      modularity: { a: modularityA, b: modularityB },
    },
    complexityComparison: {
      avgCallsRatio,
      maxCallsRatio,
      edgeDensity: { a: edgeDensityA, b: edgeDensityB },
      callConcentration: { a: concentrationA, b: concentrationB },
    },
    riskComparison: {
      distributions: {
        a: repoA.riskBreakdown,
        b: repoB.riskBreakdown,
      },
      chiSquaredDistance: riskChiSquared,
    },
  };
}

/**
 * Compute feature vector for a repository (for clustering/classification)
 */
export function computeFeatureVector(repo: RepoOverview): number[] {
  const total = repo.riskBreakdown.high + repo.riskBreakdown.medium + repo.riskBreakdown.low;

  return [
    // Size metrics (normalized by typical ranges)
    Math.log10(repo.stats.files) / 4,           // log scale, /4 for normalization
    Math.log10(repo.stats.symbols) / 5,
    Math.log10(repo.stats.modules) / 3,

    // Density metrics
    repo.stats.symbols / repo.stats.files / 50,  // symbol density
    repo.stats.modules / repo.stats.files,        // modularity
    repo.stats.callEdges / repo.stats.symbols,    // edge density

    // Complexity metrics
    repo.stats.avgCallsPerSymbol / 10,
    Math.log10(repo.stats.maxCallsInSymbol) / 3,
    (repo.stats.maxCallsInSymbol / repo.stats.avgCallsPerSymbol) / 100, // concentration

    // Risk distribution
    repo.riskBreakdown.high / total,
    repo.riskBreakdown.medium / total,
    repo.riskBreakdown.low / total,

    // Duplication
    repo.duplicates.totalClusters / repo.stats.symbols * 100,
  ];
}

/**
 * Explain what makes two repos similar or different
 */
export function explainComparison(result: ComparisonResult, repoA: RepoOverview, repoB: RepoOverview): string[] {
  const insights: string[] = [];

  // Overall
  const similarity = (result.overallSimilarity * 100).toFixed(1);
  insights.push(`Overall similarity: ${similarity}%`);

  // Highest dimension
  const dims = Object.entries(result.dimensions);
  dims.sort((a, b) => b[1] - a[1]);
  const highest = dims[0];
  const lowest = dims[dims.length - 1];

  insights.push(`Most similar in: ${highest[0]} (${(highest[1] * 100).toFixed(0)}%)`);
  insights.push(`Least similar in: ${lowest[0]} (${(lowest[1] * 100).toFixed(0)}%)`);

  // Vocabulary insights
  if (result.vocabularyOverlap.shared.length > 0) {
    insights.push(`Shared vocabulary: ${result.vocabularyOverlap.shared.slice(0, 5).join(", ")}...`);
  }

  // Complexity insights
  const concA = result.complexityComparison.callConcentration.a;
  const concB = result.complexityComparison.callConcentration.b;
  if (concA > concB * 5) {
    insights.push(`${repoA.name} has more concentrated complexity ("god functions")`);
  } else if (concB > concA * 5) {
    insights.push(`${repoB.name} has more concentrated complexity ("god functions")`);
  }

  // Size insights
  if (result.structuralComparison.sizeRatio < 0.3) {
    insights.push(`Significant size difference: ${repoB.name} is ~${Math.round(1/result.structuralComparison.sizeRatio)}x larger`);
  }

  return insights;
}
