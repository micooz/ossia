\version "2.26.0"

melody = \relative c'' {
  \key g \major \time 6/8
  d8^\markup { "G" } g b a g fis |
  e8^\markup { "Em" } g b d4 b8 |
}

accompaniment = {
  \key g \major \time 6/8
  g,8 d g b g d |
  e,8 b, e g e b, |
}

\score {
  <<
    \new Staff \with { instrumentName = "Fl." midiInstrument = "flute" } \melody
    \new Staff \with { instrumentName = "Pno." midiInstrument = "acoustic grand" } { \clef bass \accompaniment }
  >>
  \layout { }
  \midi { }
}
