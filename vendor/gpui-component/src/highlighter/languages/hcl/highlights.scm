(comment) @comment
(identifier) @variable
(attribute (identifier) @property)
(block (identifier) @type)
(string_lit) @string
(template_literal) @string
(numeric_lit) @number
(bool_lit) @boolean
(null_lit) @constant
(function_call (identifier) @function)
["for" "in" "if"] @keyword
["=" "=>" "+" "-" "*" "/" "%" "!" "==" "!=" "<" ">" "<=" ">=" "&&" "||" "?"] @operator
["{" "}" "[" "]" "(" ")"] @punctuation.bracket
["," "." ":"] @punctuation.delimiter
