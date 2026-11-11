\version "2.26.0"
Music = \relative c' { \compressMMRests { c1 | % 1
 R1*4 | % 5
} }
\score { \new Staff \Music \layout {} \midi {} }
