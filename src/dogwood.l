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

\= '='
let 'let'

and "and"
or "or"
\! "!"

\=\= '=='
\!\= '!='
\> '>'
\< '<'

if "if"
else "else"
while "while"
\{ "{"
\} "}"
\; ";"
[\t ]+ ;

\w+ "IDENT"
