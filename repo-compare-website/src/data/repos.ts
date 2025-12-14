// Static repository data extracted from semfora-engine analysis
// This data is generated automatically from semfora tool output

export interface RepoOverview {
  name: string;
  path: string;
  description: string;
  patterns: string[];
  stats: {
    files: number;
    symbols: number;
    modules: number;
    callEdges: number;
    totalCalls: number;
    avgCallsPerSymbol: number;
    maxCallsInSymbol: number;
  };
  riskBreakdown: {
    high: number;
    medium: number;
    low: number;
  };
  duplicates: {
    totalClusters: number;
    totalDuplicates: number;
  };
  topCallees: { name: string; count: number }[];
  topCallers: { hash: string; calls: number }[];
  sampleSymbols: {
    name: string;
    kind: string;
    module: string;
    risk: string;
  }[];
  moduleStats: {
    name: string;
    purpose: string;
    files: number;
    risk: string;
  }[];
}

export const daggerfallUnity: RepoOverview = {
  name: "daggerfall-unity",
  path: "/test-repos/daggerfall-unity",
  description: "Open-source recreation of The Elder Scrolls II: Daggerfall using Unity engine",
  patterns: ["CLI application", "Data serialization", "AST/code analysis"],
  stats: {
    files: 1077,
    symbols: 13451,
    modules: 335,
    callEdges: 6892,
    totalCalls: 21416,
    avgCallsPerSymbol: 3.1,
    maxCallsInSymbol: 44,
  },
  riskBreakdown: {
    high: 1024,
    medium: 10,
    low: 43,
  },
  duplicates: {
    totalClusters: 449,
    totalDuplicates: 97,
  },
  topCallees: [
    { name: "TextManager.Instance.GetLocalizedText", count: 352 },
    { name: "string.Format", count: 321 },
    { name: "DaggerfallUI.Instance.PlayOneShot", count: 221 },
    { name: "string.IsNullOrEmpty", count: 204 },
    { name: "DaggerfallUnity.Instance.TextProvider.GetRSCTokens", count: 147 },
    { name: "Path.Combine", count: 134 },
    { name: "mcp.GetMacroDataSource", count: 112 },
    { name: "UnityEngine.Random.Range", count: 111 },
    { name: "new Vector2", count: 108 },
    { name: "Debug.Log", count: 101 },
  ],
  topCallers: [
    { hash: "35df80451a7a9298", calls: 44 },
    { hash: "ef97447d4f7488dd", calls: 44 },
    { hash: "73a8079fd9959a91", calls: 40 },
    { hash: "852b245a678d98fb", calls: 37 },
    { hash: "83b253e04f4f463a", calls: 36 },
  ],
  sampleSymbols: [
    { name: "PostProcessingTests", kind: "class", module: "Tests.Runtime", risk: "low" },
    { name: "ParameterOverride", kind: "class", module: "PostProcessing.Runtime", risk: "low" },
    { name: "Update", kind: "function", module: "Game.UserInterface", risk: "high" },
    { name: "Draw", kind: "function", module: "Game.UserInterface", risk: "high" },
    { name: "RegisterCommands", kind: "function", module: "Game.TalkManager", risk: "high" },
    { name: "SetupSingleton", kind: "function", module: "Game.Questing", risk: "high" },
    { name: "ReadyCheck", kind: "function", module: "MaterialReader", risk: "high" },
    { name: "FinalizeFoe", kind: "function", module: "Game.Utility", risk: "high" },
  ],
  moduleStats: [
    { name: "config", purpose: "Configuration files", files: 14, risk: "low" },
    { name: "api", purpose: "API route handlers", files: 63, risk: "high" },
    { name: "lib", purpose: "Shared utilities and helpers", files: 24, risk: "high" },
    { name: "tests", purpose: "Test files and fixtures", files: 18, risk: "high" },
    { name: "server", purpose: "Server/service implementations", files: 6, risk: "high" },
    { name: "components", purpose: "UI components", files: 24, risk: "high" },
    { name: "other", purpose: "Other files", files: 928, risk: "high" },
  ],
};

export const nopCommerce: RepoOverview = {
  name: "nopCommerce",
  path: "/test-repos/nopCommerce",
  description: "Open-source ASP.NET Core e-commerce platform with multi-store support",
  patterns: ["AST/code analysis", "Dockerized"],
  stats: {
    files: 4143,
    symbols: 15999,
    modules: 700,
    callEdges: 6754,
    totalCalls: 31429,
    avgCallsPerSymbol: 4.7,
    maxCallsInSymbol: 795,
  },
  riskBreakdown: {
    high: 3212,
    medium: 404,
    low: 527,
  },
  duplicates: {
    totalClusters: 316,
    totalDuplicates: 179,
  },
  topCallees: [
    { name: "ArgumentNullException.ThrowIfNull", count: 878 },
    { name: "string.IsNullOrEmpty", count: 567 },
    { name: "_localizationService.GetResourceAsync", count: 520 },
    { name: "_workContext.GetCurrentCustomerAsync", count: 292 },
    { name: "_storeContext.GetCurrentStoreAsync", count: 261 },
    { name: "string.Format", count: 258 },
    { name: "RedirectToAction", count: 205 },
    { name: "_productService.GetProductByIdAsync", count: 176 },
  ],
  topCallers: [
    { hash: "308cc36eef9e1334", calls: 795 },
    { hash: "07af178f9341fbf6", calls: 485 },
    { hash: "f699d486581e897d", calls: 230 },
    { hash: "9746920566d21cc4", calls: 164 },
    { hash: "21c67e1ee01cd68a", calls: 85 },
  ],
  sampleSymbols: [
    { name: "Program", kind: "class", module: "Build", risk: "low" },
    { name: "LocalizedModelFactory", kind: "class", module: "Web.Framework.Factories", risk: "low" },
    { name: "PrepareLocalizedModelsAsync", kind: "function", module: "Web.Framework.Factories", risk: "medium" },
    { name: "HandleEventAsync", kind: "function", module: "Infrastructure.Cache", risk: "high" },
    { name: "ValidatePaymentFormAsync", kind: "function", module: "Payments", risk: "high" },
    { name: "CustomerList", kind: "function", module: "Admin.Controllers", risk: "high" },
  ],
  moduleStats: [
    { name: "api", purpose: "API route handlers", files: 237, risk: "high" },
    { name: "components", purpose: "UI components", files: 87, risk: "high" },
    { name: "tests", purpose: "Test files and fixtures", files: 209, risk: "high" },
    { name: "Libraries", purpose: "Libraries module", files: 516, risk: "high" },
    { name: "Presentation", purpose: "Presentation module", files: 1801, risk: "high" },
    { name: "database", purpose: "Database schema and migrations", files: 79, risk: "high" },
    { name: "config", purpose: "Configuration files", files: 92, risk: "low" },
    { name: "server", purpose: "Server/service implementations", files: 659, risk: "high" },
    { name: "lib", purpose: "Shared utilities and helpers", files: 22, risk: "high" },
    { name: "Plugins", purpose: "Plugins module", files: 439, risk: "high" },
  ],
};

export const repos = [daggerfallUnity, nopCommerce];
