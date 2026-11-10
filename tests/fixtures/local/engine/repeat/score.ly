\version "2.26.0"
Music = \relative c' { \repeat volta 2 { c1 | d1 | } }
\score { \new Staff \Music \layout {} \midi {} }
