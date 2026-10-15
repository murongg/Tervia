(identifier) @variable
(field (identifier) @property)
[(type) (key_type) (message_name) (enum_name) (service_name)] @type
(rpc_name) @function
(string) @string
(escape_sequence) @string.escape
[(int_lit) (float_lit)] @number
[(true) (false)] @boolean
(comment) @comment

[
  "syntax" "edition" "package" "option" "import" "public" "weak"
  "service" "rpc" "returns" "stream" "message" "enum" "oneof"
  "optional" "required" "repeated" "reserved" "to" "max"
  "extend" "extensions" "map"
] @keyword

["(" ")" "[" "]" "{" "}" "<" ">"] @punctuation.bracket
[";" "," "."] @punctuation.delimiter
"=" @operator
