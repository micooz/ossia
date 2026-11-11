\version "2.26.0"
#(define label "{ c4 | } 中文😀")
% c4 | is only a comment
Music = \relative c' { c1^\markup { "not music: d4 |" } | }
\score { \new Staff \Music \layout {} \midi {} }
