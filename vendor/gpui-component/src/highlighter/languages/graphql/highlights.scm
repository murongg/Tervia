(name) @variable
(field (name) @property)
(argument (name) @property)
(object_field (name) @property)
(named_type (name) @type)
(enum_value (name) @constant)
(operation_definition (name) @function)
(fragment_name) @function
(directive (name) @attribute)
(variable) @variable
(string_value) @string
(description) @comment.doc
[(int_value) (float_value)] @number
(boolean_value) @boolean
(null_value) @constant
(comment) @comment

[
  "query" "mutation" "subscription" "fragment" "on"
  "schema" "scalar" "type" "interface" "implements"
  "union" "enum" "input" "extend" "directive" "repeatable"
] @keyword

["(" ")" "[" "]" "{" "}"] @punctuation.bracket
[(comma) ":"] @punctuation.delimiter
["!" "=" "|" "&" "..." "@" "$"] @operator
