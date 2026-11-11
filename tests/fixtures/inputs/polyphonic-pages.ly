\version "2.26.0"
\paper { ragged-last = ##t }
Upper = \relative c' { c1 | % 1
 d1 | % 2
 \pageBreak e1 | % 3
 f1 | % 4
}
Second = \relative c' { g1 | % 1
 a1 | % 2
 b1 | % 3
 c1 | % 4
}
Lower = \relative c { c1 | % 1
 d1 | % 2
 e1 | % 3
 f1 | % 4
}
\score { \new PianoStaff <<
 \new Staff = "upper" << \context Voice = "one" { \voiceOne \Upper } \context Voice = "two" { \voiceTwo \Second } >>
 \new Staff = "lower" { \clef bass \context Voice = "three" { \Lower } }
>> \layout {} \midi {} }
