\version "2.26.0"
\header { title = "月亮 & 山" composer = "曲作者" poet = "词作者" }

melody = \relative c' {
  \key d \minor \time 3/4 \partial 4 a4 |
  \repeat volta 2 { \tuplet 3/2 { d8( e f) } <d f>4~ <d f>4 | }
  \alternative { { e2. | } { d2. | } }
}

\score {
  <<
    \new ChordNames \chordmode { s4 d2.:m/f a2.:7 d2.:m }
    \new Staff \new Voice = "sung" \melody
    \new Lyrics \lyricsto "sung" { 你 好 __ 山 a -- men }
    \new Lyrics \lyricsto "sung" { 我 看 __ 星 光 啊 }
  >>
  \layout {}
}
