; Groovy locals.scm - Variable and reference tracking
; Source: nvim-treesitter (MIT License)

; Scopes
(function_definition) @local.scope

; Parameters
(parameter
  name: (identifier) @local.definition.parameter)

; References
(identifier) @local.reference
