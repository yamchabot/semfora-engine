; Bash locals.scm - Variable and reference tracking
; Source: nvim-treesitter (MIT License)

; Scopes
(function_definition) @local.scope

; Definitions
(variable_assignment
  name: (variable_name) @local.definition.var)

(function_definition
  name: (word) @local.definition.function)

; References - captures both $VAR and ${VAR} patterns
(variable_name) @local.reference

(word) @local.reference
