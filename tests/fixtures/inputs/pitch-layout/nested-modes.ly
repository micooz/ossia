\version "2.26.0"
Music = \relative c'' { c4 \absolute { c8 \absolute { e'8 } g'4 } c4 | \relative c' { c4 d4 } c2 | }
\score { \new Staff \Music \layout {} \midi {} }
