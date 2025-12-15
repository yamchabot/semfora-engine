#!/bin/bash

# Semfora Engine MCP Call Benchmark Script
# Times various CLI calls against the Daggerfall Unity repo

set -e

# Configuration
ENGINE_BIN="${HOME}/.local/bin/semfora-engine"
TEST_REPO="/home/kadajett/Dev/Semfora_org/test-repos/daggerfall-unity"
RESULTS_FILE="benchmark-results-$(date +%Y%m%d-%H%M%S).txt"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Arrays to store timing data
declare -a CALL_NAMES
declare -a CALL_TIMES

# Timer function - returns milliseconds
time_call() {
    local name="$1"
    shift
    local start_ns=$(date +%s%N)

    # Run the command, capture output but don't display
    "$@" > /dev/null 2>&1
    local exit_code=$?

    local end_ns=$(date +%s%N)
    local elapsed_ms=$(( (end_ns - start_ns) / 1000000 ))

    CALL_NAMES+=("$name")
    CALL_TIMES+=("$elapsed_ms")

    if [ $exit_code -eq 0 ]; then
        printf "${GREEN}%-60s${NC} %7d ms\n" "$name" "$elapsed_ms"
    else
        printf "${RED}%-60s${NC} %7d ms (FAILED)\n" "$name" "$elapsed_ms"
    fi

    return $exit_code
}

# Print section header
section() {
    echo ""
    echo -e "${BLUE}═══════════════════════════════════════════════════════════════════${NC}"
    echo -e "${YELLOW}  $1${NC}"
    echo -e "${BLUE}═══════════════════════════════════════════════════════════════════${NC}"
}

# Check prerequisites
echo -e "${BLUE}Semfora Engine MCP Call Benchmark${NC}"
echo "=================================="
echo ""
echo "Engine:    $ENGINE_BIN"
echo "Test Repo: $TEST_REPO"
echo "Results:   $RESULTS_FILE"
echo ""

if [ ! -x "$ENGINE_BIN" ]; then
    echo -e "${RED}ERROR: Engine binary not found or not executable at $ENGINE_BIN${NC}"
    exit 1
fi

if [ ! -d "$TEST_REPO" ]; then
    echo -e "${RED}ERROR: Test repo not found at $TEST_REPO${NC}"
    exit 1
fi

# Ensure index exists
echo "Checking/building index..."
"$ENGINE_BIN" --dir "$TEST_REPO" --shard --summary-only > /dev/null 2>&1 || true
echo ""

# Start benchmark
START_TIME=$(date +%s)

section "Symbol Search Queries (--search-symbols)"

time_call "search_symbols: Spell (limit 30)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --search-symbols "Spell" --limit 30

time_call "search_symbols: Magic (limit 30)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --search-symbols "Magic" --limit 30

time_call "search_symbols: Effect (limit 40)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --search-symbols "Effect" --limit 40

time_call "search_symbols: SpellRecord (limit 20)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --search-symbols "SpellRecord" --limit 20

time_call "search_symbols: TargetType (limit 20)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --search-symbols "TargetType" --limit 20

time_call "search_symbols: ElementType (limit 20)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --search-symbols "ElementType" --limit 20

time_call "search_symbols: MagicSchool (limit 20)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --search-symbols "MagicSchool" --limit 20

time_call "search_symbols: EffectBundle (limit 20)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --search-symbols "EffectBundle" --limit 20

time_call "search_symbols: BaseEntityEffect (limit 20)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --search-symbols "BaseEntityEffect" --limit 20

time_call "search_symbols: MagicSkills (limit 15)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --search-symbols "MagicSkills" --limit 15

time_call "search_symbols: Damage* (limit 20)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --search-symbols "Damage*" --limit 20

time_call "search_symbols: * high risk (limit 30)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --search-symbols "*" --risk "high" --limit 30

time_call "search_symbols: * fn kind (limit 50)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --search-symbols "*" --kind "fn" --limit 50

time_call "search_symbols: GetBonusOrPenaltyByEnemyType" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --search-symbols "GetBonusOrPenaltyByEnemyType" --limit 5

section "List Symbols by Module (--list-symbols)"

time_call "list_symbols: Game.MagicAndEffects.Effects.Destruction" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --list-symbols "Game.MagicAndEffects.Effects.Destruction" --limit 30

time_call "list_symbols: Game.MagicAndEffects.Effects.Restoration" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --list-symbols "Game.MagicAndEffects.Effects.Restoration" --limit 30

time_call "list_symbols: Game.MagicAndEffects.Effects.Illusion" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --list-symbols "Game.MagicAndEffects.Effects.Illusion" --limit 30

time_call "list_symbols: Game.MagicAndEffects.Effects.Alteration" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --list-symbols "Game.MagicAndEffects.Effects.Alteration" --limit 30

time_call "list_symbols: Game.MagicAndEffects.Effects.Thaumaturgy" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --list-symbols "Game.MagicAndEffects.Effects.Thaumaturgy" --limit 30

time_call "list_symbols: Game.MagicAndEffects.Effects.Mysticism" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --list-symbols "Game.MagicAndEffects.Effects.Mysticism" --limit 30

time_call "list_symbols: Game.Formulas (fn kind)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --list-symbols "Game.Formulas" --kind "fn" --limit 50

time_call "list_symbols: Game (fn kind)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --list-symbols "Game" --kind "fn" --limit 50

section "Repository Overview (--get-overview)"

time_call "get_overview" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --get-overview

section "Call Graph (--get-call-graph)"

time_call "get_call_graph" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --get-call-graph

section "Duplicate Detection (--find-duplicates)"

time_call "find_duplicates (threshold 0.85, limit 20)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --find-duplicates --duplicate-threshold 0.85 --limit 20

section "Raw Search (--raw-search)"

time_call "raw_search: Magic schools regex" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --raw-search "Destruction|Restoration|Alteration|Mysticism|Thaumaturgy|Illusion" --file-types "cs" --limit 30

time_call "raw_search: enum MagicSkills" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --raw-search "enum MagicSkills" --file-types "cs" --limit 10

time_call "raw_search: SpellRecord class" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --raw-search "class SpellRecord" --file-types "cs" --limit 10

section "Source Code Reads (--get-source)"

time_call "get_source: DaggerfallSpellReader.cs (24-120)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --get-source "Assets/Scripts/Utility/DaggerfallSpellReader.cs" --start-line 24 --end-line 120 --context 0

time_call "get_source: SpellRecord.cs (22-95)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --get-source "Assets/Scripts/API/Save/SpellRecord.cs" --start-line 22 --end-line 95 --context 0

time_call "get_source: MagicAndEffectsEnums.cs (1-100)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --get-source "Assets/Scripts/Game/MagicAndEffects/MagicAndEffectsEnums.cs" --start-line 1 --end-line 100 --context 0

time_call "get_source: EntityEffect.cs (253-400)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --get-source "Assets/Scripts/Game/MagicAndEffects/EntityEffect.cs" --start-line 253 --end-line 400 --context 0

time_call "get_source: MagicAndEffectsStructs.cs (1-160)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --get-source "Assets/Scripts/Game/MagicAndEffects/MagicAndEffectsStructs.cs" --start-line 1 --end-line 160 --context 0

time_call "get_source: DFCareer.cs (506-520)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --get-source "Assets/Scripts/API/DFCareer.cs" --start-line 506 --end-line 520 --context 2

time_call "get_source: DrainLuck.cs (1-50)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --get-source "Assets/Scripts/Game/MagicAndEffects/Effects/Destruction/DrainLuck.cs" --start-line 1 --end-line 50 --context 0

time_call "get_source: DrainStrength.cs (1-50)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --get-source "Assets/Scripts/Game/MagicAndEffects/Effects/Destruction/DrainStrength.cs" --start-line 1 --end-line 50 --context 0

time_call "get_source: HealPersonality.cs (1-50)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --get-source "Assets/Scripts/Game/MagicAndEffects/Effects/Restoration/HealPersonality.cs" --start-line 1 --end-line 50 --context 0

time_call "get_source: DrainEffect.cs (1-80)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --get-source "Assets/Scripts/Game/MagicAndEffects/Effects/Destruction/DrainEffect.cs" --start-line 1 --end-line 80 --context 0

time_call "get_source: FormulaHelper.cs (538-650)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --get-source "Assets/Scripts/Game/Formulas/FormulaHelper.cs" --start-line 538 --end-line 650 --context 0

time_call "get_source: FormulaHelper.cs (650-723)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --get-source "Assets/Scripts/Game/Formulas/FormulaHelper.cs" --start-line 650 --end-line 723 --context 0

time_call "get_source: FormulaHelper.cs (993-1057)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --get-source "Assets/Scripts/Game/Formulas/FormulaHelper.cs" --start-line 993 --end-line 1057 --context 0

section "Semantic Search (--semantic-search)"

time_call "semantic_search: damage calculation" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --semantic-search "damage calculation" --limit 20

time_call "semantic_search: spell effect" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --semantic-search "spell effect" --limit 20

time_call "semantic_search: magic resistance" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --semantic-search "magic resistance" --limit 20

section "File Symbols (--file-symbols)"

time_call "file_symbols: FormulaHelper.cs" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --file-symbols "Assets/Scripts/Game/Formulas/FormulaHelper.cs" --kind "fn" --limit 50

time_call "file_symbols: EntityEffect.cs" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --file-symbols "Assets/Scripts/Game/MagicAndEffects/EntityEffect.cs" --kind "fn" --limit 50

section "Get Callers (--get-callers)"

# Note: These use symbol hashes from the index - you may need to update them
# after regenerating the index for your specific repo
time_call "get_callers: sample symbol (depth 1)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --search-symbols "CalculateAttackDamage" --limit 1

# Once you have a hash, you can use it like:
# time_call "get_callers: CalculateAttackDamage" \
#     "$ENGINE_BIN" --dir "$TEST_REPO" --get-callers "<HASH>" --depth 2 --limit 20

section "Full Index Generation (--shard)"

time_call "generate_index (full shard)" \
    "$ENGINE_BIN" --dir "$TEST_REPO" --shard --summary-only

# Calculate totals
section "Summary"

TOTAL_TIME=0
for time in "${CALL_TIMES[@]}"; do
    TOTAL_TIME=$((TOTAL_TIME + time))
done

END_TIME=$(date +%s)
WALL_TIME=$((END_TIME - START_TIME))

echo ""
echo "Total calls:      ${#CALL_NAMES[@]}"
echo "Total time:       ${TOTAL_TIME} ms"
echo "Wall clock time:  ${WALL_TIME} s"
echo "Average per call: $((TOTAL_TIME / ${#CALL_NAMES[@]})) ms"
echo ""

# Find slowest calls
echo -e "${YELLOW}Top 5 Slowest Calls:${NC}"
# Create indexed array for sorting
for i in "${!CALL_TIMES[@]}"; do
    echo "${CALL_TIMES[$i]} ${CALL_NAMES[$i]}"
done | sort -rn | head -5 | while read time name; do
    printf "  %7d ms  %s\n" "$time" "$name"
done

# Write results to file
{
    echo "Semfora Engine Benchmark Results"
    echo "================================"
    echo "Date: $(date)"
    echo "Engine: $ENGINE_BIN"
    echo "Test Repo: $TEST_REPO"
    echo ""
    echo "Results:"
    echo "--------"
    for i in "${!CALL_NAMES[@]}"; do
        printf "%-60s %7d ms\n" "${CALL_NAMES[$i]}" "${CALL_TIMES[$i]}"
    done
    echo ""
    echo "Summary:"
    echo "--------"
    echo "Total calls:      ${#CALL_NAMES[@]}"
    echo "Total time:       ${TOTAL_TIME} ms"
    echo "Wall clock time:  ${WALL_TIME} s"
    echo "Average per call: $((TOTAL_TIME / ${#CALL_NAMES[@]})) ms"
} > "$RESULTS_FILE"

echo ""
echo -e "${GREEN}Results saved to: $RESULTS_FILE${NC}"
