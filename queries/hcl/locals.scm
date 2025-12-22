; HCL/Terraform locals.scm - Variable and reference tracking
; Note: Created for semfora-engine (HCL doesn't have official locals.scm)

; Scopes - blocks create scopes
(block) @local.scope

; Definitions
; Variable definitions: variable "name" { ... }
(block
  (identifier) @_type
  (string_lit) @local.definition.var
  (#eq? @_type "variable"))

; Local definitions: locals { name = value }
(block
  (identifier) @_type
  (body
    (attribute
      (identifier) @local.definition.var))
  (#eq? @_type "locals"))

; Output definitions: output "name" { ... }
(block
  (identifier) @_type
  (string_lit) @local.definition.var
  (#eq? @_type "output"))

; Resource/data definitions
(block
  (identifier) @_type
  (string_lit) @_resource_type
  (string_lit) @local.definition.var
  (#match? @_type "^(resource|data)$"))

; Module definitions
(block
  (identifier) @_type
  (string_lit) @local.definition.var
  (#eq? @_type "module"))

; References
(identifier) @local.reference

; Variable references: var.name, local.name, etc.
(get_attr
  (variable_expr
    (identifier) @_prefix)
  (get_attr_key
    (identifier) @local.reference)
  (#match? @_prefix "^(var|local|module|data)$"))
