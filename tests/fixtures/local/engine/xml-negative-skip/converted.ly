\version "2.26.0"
% automatically converted by musicxml2ly from input.musicxml
\pointAndClickOff

\header {
  title = "Ossia synthetic"
}
\layout {
  \context {
    \Staff
    printKeyCancellation = ##f
  }
  \context {
    \Score
    autoBeaming = ##f
  }
}
PartPOneVoiceOne = \relative c' {
  \clef "treble" \numericTimeSignature \time 4/4 \key c \major c1 d1*1/2 }


% The score definition
\score {
  <<
    \new Staff = "P1" <<
      \set Staff.instrumentName = "Synthetic"
      \context Staff <<
        \override Staff.BarLine.allow-span-bar = ##f
        \mergeDifferentlyDottedOn
        \mergeDifferentlyHeadedOn
        \context Voice = "PartPOneVoiceOne" {
          \PartPOneVoiceOne
        }
      >>
    >>
  >>
  \layout {}
  % To create MIDI output, uncomment the following line:
  % \midi { \tempo 4 = 100 }
}

