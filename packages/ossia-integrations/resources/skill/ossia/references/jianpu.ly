% ossia:notation=jianpu
\version "2.26.0"

% 邓垚《诀别书》，据弹琴吧精编版首调简谱（曲谱编号 92933）录入。

\header {
  title = "诀别书"
  tagline = ##f
}

#(set-global-staff-size 17)
\paper {
  #(set-paper-size "a4")
  ragged-last-bottom = ##t
  system-system-spacing = #'((basic-distance . 20)
                             (minimum-distance . 20)
                             (padding . 3)
                             (stretchability . 12))
}

rightHand = {
  \key f \major
  \time 4/4
  \tempo 4 = 120

  d''8( a''16 g'' a''8) r8 r4 r4 |
  c''8( g''16 f'' g''8) r8 r4 r4 |
  c''8( g''16 f'' g''8) r8 r4 r4 |
  c'''8( f''16 e'' f''8) r8 r4 r4 | \break

  d''8( a''16 g'' a''2) r4 |
  d'''8( g''16 f'' g''2) r4 |
  e'''8( g''16 f'' g''2) r4 |
  <a'' f'''>8( <g'' e'''> <d'' f'' a'' d'''>2.) | \break

  d''8( a''16 g'' a''8 g'' a'' d''' a'' g'' |
  a''16 g'' a'' g'' a''8 g''
    c''8) <c'' g'' c'''>16 <c'' g'' c'''>~ <c'' g'' c'''> g'' f'' c'' |
  c''8( g''16 f'' g''8 f'' g'' c''' c''' g'' |
  g''16 a'' g'' a'' g''8 e'' f''2) | \break

  d''8( a''16 g'' a''8 g'' a'' d''' a'' g'' |
  a''16 g'' a'' g'' a''8 g'' c'') <c'' c'''> <f'' f'''> <e'' e'''> |
  c''8( g''16 f'' g''8 f'' g'' c''' c''' e''' |
  d'''16 e''' d''' e''' d'''8 c''' d''' a'' <a'' d'''> <a'' e'''>) | \break

  % 17–19
  \ottava #1
  <f'' bes'' d''' f'''>4. <f'' bes'' d'''>8~ <f'' bes'' d'''>4 <a'' a'''> |
  <bes'' des''' bes'''>4. <bes'' d''' g''' bes'''>8~ <bes'' d''' g''' bes'''>2 |
  \ottava #0
  <f'' a'' e'''>4.\arpeggio <c'' f'' a'' c'''>8~ <c'' f'' a'' c'''>2 |

  % 20–23
  <c'' d'' a''>4.\arpeggio <c'' d'' g''>8~ <c'' d'' g''>2 |
  <bes' d'' g'' bes''>2.\arpeggio <a' a''>8 <bes' bes''> | \break
  <cis'' e'' c'''>4. <c'' e'' bes''>8~ <c'' e'' bes''>4 <c'' e'' a''> |
  <a' c'' g''>4. <a' c'' f''>8~ <a' c'' f''>4 <a' c'' g''> |

  % 24–27
  <a' ees'' a''>4 <a' e'' fis''> <a' e'' g''> <a' e'' a''> |
  <f' bes' d'' f''>4. <f' bes' d''>8~ <f' bes' d''>4 <a' a''> |
  <bes' des'' bes''>4. <bes' d'' g'' bes''>8~ <bes' d'' g'' bes''>2 | \pageBreak
  <c'' e'' bes''>4.\arpeggio <c'' e'' a''>8~ <c'' e'' a''>2 |

  % 28–31
  <a' d'' g''>4.\arpeggio <d' a' d''>8~ <d' a' d''>4 d'' |
  <bes' d'' a''>8 <bes' d'' g''> r <bes' d''>~ <bes' d''>2 |
  <g' cis'' f''>8 <g' c'' e''> r <cis' e' a'>~ <cis' e' a'>2 |
  <f' a' c'' e''>2.\arpeggio <f a e'>4 | \break

  % 32–35
  <f' a' d''>1 |
  <f' a' e''>8 <f' d''> <f' a'> <f' a' e''> <f' d''> <f' a'> <f' a' d''> <f' a' e''> |
  <d'' f''>8 e'' a' <a' f''> <f' e''> <f' a'> <f' a' e''> <f' a' f''> |
  <a' c'' g''>8 <a' f''> <a' c''> <a' c'' g''> <a' f''> <a' c''> <a' c'' f''> <a' c'' g''> |

  % 36–39
  <a' d'' a''>1 | \break
  \ottava #1
  <f'' a'' e'''>8 <f'' d'''> <f'' a''> <f'' a'' e'''>
    <f'' d'''> <f'' a''> <f'' a'' d'''> <f'' a'' e'''> |
  <a'' c''' f'''>8 <a'' e'''> <a'' c'''> <a'' c''' f'''>
    <a'' e'''> <a'' c'''> <f'' a'' e'''> <f'' a'' f'''> |
  <a'' c''' g'''>8 <a'' f'''> <a'' c'''> <a'' c''' g'''>-!
    <a'' f'''>-! <a'' c'''> <a'' c''' f'''>-! <a'' c''' e'''> |

  % 40–43
  <a'' d'''>1 | \break
  \ottava #0
  <d'' f'' a''>8 <a'' d'''>16 <a'' d'''> <a'' d'''>8 <d'' a''>16 <d'' a''>
    <d'' a''>8 <d'' f''>16 <d'' f''> <d'' f''>8 <d'' a''> |
  <d'' a''>8 <c'' g''>16 <c'' g''> <c'' g''>8 <c'' fis''> <c'' g''>2 |
  <c'' e'' g''>8 <g'' c'''>16 <g'' c'''> <g'' c'''>8 <c'' g''>16 <c'' g''>
    <c'' g''>8 <c'' e''>16 <c'' e''> <c'' e''>8 <c'' g''> |

  % 44–47
  <a' c'' g''>8 <a' c'' f''>16 <a' c'' f''> <a' c'' f''>8 <a' c'' e''> <a' d'' f''>2 | \break
  <d'' f'' c'''>8\arpeggio bes''8 f'' c'''~ c''' bes''4. |
  <cis'' e'' bes''>4.\arpeggio <c'' e'' a''>8~ <c'' e'' a''>4 e'' |
  <a' c'' e''>1\arpeggio |

  % 48–52
  d''1 | \break
  \ottava #1
  d'''4 a'''8 g''' a'''4 g''' |
  a'''4 d'''' a''' g''' |
  a'''8 g''' a''' g''' a'''4 g''' |
  c''''1 | \ottava #0 \bar "|."
}

leftHand = {
  \clef bass
  \key f \major
  \time 4/4

  bes8 <d' f' a'>4 <d' f' a'>8 bes <d' f' a'>4 d'8 |
  bes8 <c' e' g'>4 <c' e' g'>8 bes <c' e' g'>4 <c' e' g'>8 |
  a8 <c' e' g'>4 <c' e' g'>8 a <c' e' g'>4 <c' e' g'>8 |
  d8 <a c'>4 <a c'>8 d <c' e'> c <c' e'> |

  bes8 <d' f' a'>4 <d' f' a'>8 bes <d' f' a'>4 <d' f' a'>8 |
  bes8 <c' e' g'>4 <c' e' g'>8 bes <c' e' g'>4 <c' e' g'>8 |
  a8 <c' e' g'>4 <c' e' g'>8 a <c' e' g'>4 a,8 |
  d,4 <d, a c'> <d, d> <c, c> |

  <bes,, bes,>8 <d' f' a'>4 <d' f' a'>8 bes f'4 <d' f' a'>8 |
  bes8 <c' e' g'>4 <c' e' g'>8 bes-! r8 e'-! c'-! |
  a8 <c' e' g'>4 <c' e' g'>8 a <c' e' g'>4 a,8 |
  d,4 <d, a c'> <d, d> <c, c> |

  <bes,, bes,>8 <d' f' a'>4 <d' f' a'>8 bes <d' f' a'>4 <d' f' a'>8 |
  bes8 <c' e' g'>4 <c' e' g'>8 bes-! g' e'-! c'-! |
  a8 <c' e' g'>4 <c' e' g'>8 a <c' e' g'>4 a,8 |
  d,4 <d, a c'> <d, d>8 <c, c> <bes,, bes,> <a,, a,> |

  % 17–19
  g,4 bes8 d' f' d' bes g, |
  c,8 c e g bes g e c |
  <f,, f,>8 f a c' e' c' a f |

  % 20–23
  <bes,, bes,>4 d'8 f' a' f' d' bes |
  e,4. <e, g bes d'>8~ <e, g bes d'>4. e,8 |
  <a,, a,>4 cis8 e g e c a,, |
  d,4. <d, a c'>8~ <d, a c'>4 a, |

  % 24–27
  <d, d>4 <a c' fis'> <d, d>8 <c, c> <bes,, bes,> <a,, a,> |
  <g,, g,>4 bes8 d' f' d' bes g, |
  c,8 c e g bes g e c |
  f,8 f a c' e' c' a f |

  % 28–31
  <bes,, bes,>4. <bes d' f' a'>8~ <bes d' f' a'>4 bes, |
  e,4. <e, g bes d'>8~ <e, g bes d'>4 e, |
  <a,, a,>4. <a cis' e' g'>8~ <a cis' e' g'>4 a, |
  d,4 a e' a |

  % 32–35
  <d c'>2 <c, c> |
  <bes,, bes,>8 r r <bes,, bes,> r r <bes,, bes,> r |
  <c, c>8 r r <c, c> r r <c, c> r |
  <d, d>8 r r <d, d> r r <d, d> r |

  % 36–39
  <d, a, d>4 <c, g, c> <d, d> <c, c> |
  <bes,, bes,>8 r r <bes,, bes,> r r <bes,, bes,> r |
  <c, c>8 r r <c, c> r r <c, c> r |
  <d, d>8 r r <d, d> r r <d, d> r |

  % 40–43
  <d, d>4 <d, a c'> <d, d> <c, c> |
  <bes,, bes,>8 r r <f' a'>4 r8 <f' a'>4 |
  bes8 <c' e' g'>4 <c' e' g'>8 bes-! g'-! e'-! c'-! |
  a8 <c' e' g'>4 <c' e' g'>8 a <c' e' g'>4 a,8 |

  % 44–47
  d,4 <a c'> d d, |
  g,8 d g bes f' bes4 e,8 |
  a,4 a g' r8 a, |
  d4 a e' f'~ |

  % 48–52
  f'2 c |
  bes,2 <f a> |
  <d' f'>1 |
  <c e>1~ |
  <c e>1 |
}

\score {
  \new PianoStaff <<
    \new Staff = "right" \rightHand
    \new Staff = "left" \leftHand
  >>
  \layout { }
  \midi { }
}
