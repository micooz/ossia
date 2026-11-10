\version "2.26.0"
trackBchannelA = \relative c' { c1 | d1 | }
trackBchannelB = \relative c { c1 | d1 | }
trackA = << \context Voice = voiceA \trackBchannelA >>
trackB = << \context Voice = voiceB \trackBchannelB >>
\score { << \context Staff=trackB \trackA \context Staff=trackB \trackB >> \layout {} \midi {} }
