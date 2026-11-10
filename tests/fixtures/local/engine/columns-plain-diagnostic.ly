\version "2.26.0"
Music = \relative c' { \mark \markup { "中文😀" }  \unknownProbe c4 d4 e4 f4 | }
\score { \new Staff \Music \layout {} \midi {} }
