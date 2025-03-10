%%
\-?[0-9]+ "INT"
true|false "BOOL"
\_ "BLANK"

\+ "+"
\- "-"
\* "*"
\/ "/"
\^ "^"
\% "%"

\( "("
\) ")"

and "and"
or "or"
\! "!"

\=\= '=='
\!\= '!='
\> '>'
\< '<'

\= '='
let 'let'

if "if"
else "else"
while "while"
\{ "{"
\} "}"
\; ";"
[\t ]+ ;

\w+ "IDENT"
