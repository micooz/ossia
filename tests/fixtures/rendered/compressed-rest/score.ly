\version "2.26.0"
Upper = \relative c' { c1 | % 1
 R1*4 | % 5
}
\score { \new Staff = "upper" \Upper \layout {} \midi {} }
