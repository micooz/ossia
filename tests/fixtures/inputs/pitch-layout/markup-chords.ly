\version "2.26.0"
Music = \relative c' { c4^\markup { \bold { "中文😀 c8 |" } } <e g>4 r4 f4 | g2 c,2 | }
\score { \new Staff \Music \layout {} \midi {} }
