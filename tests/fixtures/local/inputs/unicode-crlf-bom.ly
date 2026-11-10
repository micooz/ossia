\version "2.26.0"
% 中文😀
Music = \relative c' { c4^\markup { "中文😀" }	d4 e4 f4 | }
\score { \new Staff \Music \layout {} \midi {} }
