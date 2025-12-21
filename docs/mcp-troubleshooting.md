# MCP Troubleshooting Guide

Common issues and solutions when using semfora-engine MCP tools.

---

## Quick Troubleshooting Table

| Symptom | Likely Cause | Solution |
|---------|--------------|----------|
| "Module not found" | Wrong module name | Get exact name from `get_overview` |
| "Symbol not found" | Stale hash or typo | Re-search, verify hash format |
| "Index stale" | Files changed | Run `index()` then retry |
| Output truncated | Result too large | Use pagination, filters, or summary_only |
| File too large notice | File >3000 lines | Use focus mode (start_line/end_line) |
| Empty search results | Query too specific | Broaden query, check language |
| Tool returns error | Various | Check parameters, see specific error |

---

## Index Issues

### "Index is stale"

**Cause:** Files have been modified since last index.

**Solution:**
```json
// Refresh the index
{ "tool": "index", "operation": "refresh" }

// Then retry your operation
```

### "Index not found"

**Cause:** Repository hasn't been indexed yet.

**Solution:**
```json
// Create initial index
{ "tool": "index", "operation": "refresh", "path": "/path/to/repo" }
```

### Index takes too long

**Cause:** Large repository or slow disk.

**Solution:**
- Let it complete (first index is slower)
- Subsequent indexes are incremental
- Use `get_context()` to check index status

---

## Module Issues

### "Module 'X' not found"

**Cause:** Module name doesn't match exactly.

**Common mistakes:**
- `database` instead of `services.database`
- `db` instead of `semfora_pm.db`
- Guessing instead of copying from get_overview

**Solution:**
```json
// First, get exact module names
{ "tool": "get_overview" }

// Then use EXACT name from output
{ "tool": "validate", "module": "semfora_pm.db" }  // Correct!
{ "tool": "validate", "module": "database" }       // Wrong!
```

### Module exists but returns empty

**Cause:** Module has no symbols or wasn't parsed.

**Solution:**
```json
// Try analyzing the directory directly
{ "tool": "analyze", "path": "/path/to/module/dir" }
```

---

## Search Issues

### Search returns no results

**Possible causes:**
1. Query too specific
2. Symbol doesn't exist
3. Wrong language/syntax

**Solutions:**
```json
// Try broader query
{ "tool": "search", "query": "auth" }  // Instead of "authenticateUser"

// Try different mode
{ "tool": "search", "query": "auth", "mode": "raw" }

// Check if file is indexed
{ "tool": "get_context" }
```

### Wildcard search doesn't work

**Cause:** Wildcards (*) are not fully supported.

**Solution:**
```json
// Don't use wildcards
{ "tool": "search", "query": "handleError" }  // Not "handle*"

// Use raw mode for pattern matching
{ "tool": "search", "query": "handle", "mode": "raw" }
```

---

## Large File Issues

### "File too large" notice

**Cause:** File exceeds 3000 lines or 500KB.

**Solution: Use focus mode**
```json
// Get symbol list first
{ "tool": "get_file", "file_path": "path/to/large/file.tsx" }

// Then analyze specific section
{
  "tool": "analyze",
  "path": "path/to/large/file.tsx",
  "start_line": 100,
  "end_line": 300
}
```

### Analysis returns too many symbols

**Cause:** File has >50 symbols.

**Solution:**
```json
// Use symbols_only mode for overview
{
  "tool": "analyze",
  "path": "path/to/file.tsx",
  "output_mode": "symbols_only"
}

// Then focus on specific symbols
{ "tool": "search", "query": "specificFunction fileName" }
```

---

## Output Truncation

### Response cut off mid-content

**Cause:** Output exceeded ~25k token limit.

**Solutions:**

**For analyze_diff:**
```json
// Use summary first
{ "tool": "analyze_diff", "base_ref": "main", "summary_only": true }

// Then paginate
{ "tool": "analyze_diff", "base_ref": "main", "limit": 20, "offset": 0 }
```

**For find_duplicates:**
```json
// Reduce limit and increase threshold
{ "tool": "find_duplicates", "limit": 20, "threshold": 90 }
```

**For get_callgraph:**
```json
// Use summary mode
{ "tool": "get_callgraph", "summary_only": true }

// Or focus on specific symbol
{ "tool": "get_callgraph", "symbol_hash": "abc123:def456" }
```

### General truncation prevention

1. **Always use `summary_only: true` first** for large results
2. **Use pagination** with limit/offset
3. **Add filters** (module, symbol_hash, threshold)
4. **Never retry same query** - it will truncate again

---

## Symbol Hash Issues

### "Symbol not found" with hash

**Possible causes:**
1. Hash from stale index
2. Hash format incorrect
3. Symbol was deleted

**Solution:**
```json
// Verify hash format (should be shard:hash)
// Correct: "abc123:def456789012"
// Wrong: "def456789012" (missing shard)

// Re-search if stale
{ "tool": "search", "query": "symbolName" }
```

### Hash from search doesn't work in get_symbol

**Cause:** Using wrong field from search result.

**Solution:**
```json
// Use the 'hash' field, not 'id' or 'name'
// Search returns: { "hash": "abc123:def456", "name": "myFunc", ... }
{ "tool": "get_symbol", "symbol_hash": "abc123:def456" }
```

---

## Validation Issues

### "validate() needs scope"

**Cause:** No module, file_path, or symbol_hash provided.

**Solution:**
```json
// Provide at least one scope
{ "tool": "validate", "module": "api" }
// OR
{ "tool": "validate", "file_path": "src/api.ts" }
// OR
{ "tool": "validate", "symbol_hash": "abc123:def456" }
```

### Validation returns too many results

**Solution:**
```json
// Reduce limit
{ "tool": "validate", "module": "api", "limit": 20 }
```

---

## Language Detection Issues

### "Unsupported file type"

**Cause:** File extension not recognized.

**Supported extensions:**
- TypeScript: `.ts`, `.tsx`, `.mts`, `.cts`
- JavaScript: `.js`, `.jsx`, `.mjs`, `.cjs`
- Python: `.py`, `.pyi`
- Rust: `.rs`
- Go: `.go`
- Java: `.java`
- C#: `.cs`
- C/C++: `.c`, `.h`, `.cpp`, `.hpp`
- And more...

**Solution:**
```json
// Check supported languages
{ "tool": "get_languages" }
```

---

## Performance Issues

### Tools running slowly

**Possible causes:**
1. First index (slower than incremental)
2. Very large repository
3. Disk I/O bottleneck

**Solutions:**
- Wait for initial index to complete
- Use focused queries instead of repo-wide
- Check `get_context()` for index status

### Context filling up quickly

**Causes:**
1. Not using pagination
2. Not using summary_only
3. Loading too many tools
4. Requesting full output from large files

**Solutions:**
```json
// Use summary mode
{ "tool": "analyze_diff", "summary_only": true }

// Use pagination
{ "tool": "find_duplicates", "limit": 20, "offset": 0 }

// Focus on specific files
{ "tool": "analyze", "path": "specific/file.ts" }
```

---

## Error Messages Reference

| Error Message | Meaning | Solution |
|--------------|---------|----------|
| "Path not found" | File/directory doesn't exist | Check path spelling |
| "Unsupported file type" | File extension not recognized | Check `get_languages` |
| "Failed to read file" | Permission or encoding issue | Check file access |
| "Analysis failed" | Parser error | Try different file |
| "Module 'X' not found" | Wrong module name | Copy from `get_overview` |
| "start_line exceeds file length" | Invalid line range | Check file line count |
| "Index stale" | Files changed | Run `index()` |

---

## Best Practices Summary

1. **Always start with `get_context()`** - verify index status
2. **Copy module names exactly** - never guess
3. **Use pagination** for any result >20 items
4. **Use summary_only first** for large operations
5. **Call get_callers before refactoring** - verify impact
6. **Use focus mode** for files >2000 lines
7. **Batch MCPSearch calls** - don't load sequentially
8. **Don't retry truncated queries** - add filters/pagination first
