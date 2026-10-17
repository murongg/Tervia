; Capture every attribute name: testing start-tag text mistakes data-lang or
; quoted values for a lang attribute and silently disables default highlighting.
; Anchor only the start; a trailing anchor prevents multiple attributes matching.
((script_element
  (start_tag
    (tag_name) .
    [(attribute (attribute_name) @_name) (directive_attribute)]*)
  (raw_text) @injection.content)
 (#not-eq? @_name "lang")
 (#set! injection.language "javascript"))

((style_element
  (start_tag
    (tag_name) .
    [(attribute (attribute_name) @_name) (directive_attribute)]*)
  (raw_text) @injection.content)
 (#not-eq? @_name "lang")
 (#set! injection.language "css"))

((script_element
  (start_tag
    (attribute
      (attribute_name) @_name
      [(quoted_attribute_value (attribute_value) @injection.language)
       (attribute_value) @injection.language]))
  (raw_text) @injection.content)
 (#eq? @_name "lang"))

((style_element
  (start_tag
    (attribute
      (attribute_name) @_name
      [(quoted_attribute_value (attribute_value) @injection.language)
       (attribute_value) @injection.language]))
  (raw_text) @injection.content)
 (#eq? @_name "lang"))

((interpolation (raw_text) @injection.content)
 (#set! injection.language "typescript"))

((directive_attribute
  (quoted_attribute_value (attribute_value) @injection.content))
 (#set! injection.language "typescript"))
