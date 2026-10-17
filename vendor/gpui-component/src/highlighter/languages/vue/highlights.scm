; Vue's published queries inherit HTML rules; Tree-sitter does not expand that inheritance.
(tag_name) @tag
(attribute_name) @attribute
(quoted_attribute_value) @string
(comment) @comment
(directive_name) @attribute
(directive_value) @property
(directive_modifier) @function
(dynamic_directive_inner_value) @variable
["<" ">" "</" "/>" "[" "]"] @punctuation.bracket
["=" ":" "." "@"] @operator
(interpolation) @embedded
