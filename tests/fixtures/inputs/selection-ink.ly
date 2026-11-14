\version "2.26.0"
\header {
  title = "染色测试"
  subtitle = \markup \column { "中文与多行文字" "第二行" }
  composer = "作者"
  copyright = "版权"
}
Music = {
  \time 4/4
  \grace { b16-. } <c' e'>8-1-. [ d'8 ] ees'8. f'16 g'2 ~ \bar "||" \break
  g'1 ~ \bar ":|." \pageBreak
  g'1 |
  \afterGrace a'2 { b'16 } c''2 |
  <d''-2 f''-3>1 \bar "|."
}
\score { \new Staff \Music \layout {} \midi {} }
