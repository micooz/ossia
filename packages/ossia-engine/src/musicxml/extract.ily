% The scratch event stream is JSON Lines. Positions and lengths are exact whole-note fractions.
#(define ossia-xml-port (open-output-file "notation.jsonl"))
#(define ossia-xml-contexts (make-hash-table))
#(define ossia-xml-next-id 0)
#(define ossia-xml-books 0)

#(define (ossia-xml-json value)
   (let ((out ossia-xml-port))
     (cond
      ((boolean? value) (display (if value "true" "false") out))
      ((integer? value) (display value out))
      ((symbol? value) (ossia-xml-json (symbol->string value)))
      ((string? value)
       (write-char #\" out)
       (string-for-each
        (lambda (c)
          (cond
           ((char=? c #\") (display "\\\"" out))
           ((char=? c #\\) (display "\\\\" out))
           ((< (char->integer c) 32) (format out "\\u~4,'0x" (char->integer c)))
           (else (write-char c out))))
        value)
       (write-char #\" out))
      ((list? value)
       (display "[" out)
       (unless (null? value)
         (ossia-xml-json (car value))
         (for-each (lambda (item) (display "," out) (ossia-xml-json item)) (cdr value)))
       (display "]" out))
      (else (ly:error "unsupported Ossia MusicXML event value")))))

#(define (ossia-xml-line . fields)
   (ossia-xml-json fields)
   (newline ossia-xml-port))

#(define (ossia-xml-text value)
   (cond ((string? value) value)
         ((and (markup? value) (not (null? value))) (markup->string value))
         (else "")))

#(define (ossia-xml-context ctx)
   (if (or (not (ly:context? ctx)) (eq? (ly:context-name ctx) 'Global))
       0
       (or (hashq-ref ossia-xml-contexts ctx)
           (let* ((parent (ossia-xml-context (ly:context-parent ctx)))
                  (id (+ ossia-xml-next-id 1)))
             (set! ossia-xml-next-id id)
             (hashq-set! ossia-xml-contexts ctx id)
             (ossia-xml-line 'context id parent (ly:context-name ctx) (ly:context-id ctx))
             id))))

#(define (ossia-xml-origin event)
   (let ((origin (cond ((ly:stream-event? event) (ly:event-property event 'origin #f))
                       ((ly:music? event) (ly:music-property event 'origin #f))
                       (else #f))))
     (if (ly:input-location? origin)
         (let ((loc (ly:input-file-line-char-column origin)))
           (if (string=? (basename (car loc)) "score.ly")
               (list (cadr loc) (cadddr loc)) '()))
         '())))

#(define (ossia-xml-fraction n) (list (numerator n) (denominator n)))
#(define (ossia-xml-time ctx) (ly:moment-main (ly:context-current-moment ctx)))
#(define (ossia-xml-moment ctx)
   (let ((moment (ly:context-current-moment ctx)))
     (list (ossia-xml-fraction (ly:moment-main moment))
           (ossia-xml-fraction (ly:moment-grace moment)))))
#(define (ossia-xml-emit kind ctx event . fields)
   (ossia-xml-line kind (ossia-xml-context ctx) (ossia-xml-moment ctx)
                   (ossia-xml-origin event) fields))

#(define (ossia-xml-pitch p)
   (list (ly:pitch-notename p) (+ 4 (ly:pitch-octave p))
         (ossia-xml-fraction (* 2 (ly:pitch-alteration p)))))

#(define (ossia-xml-duration e)
   (let ((d (ly:event-property e 'duration)))
     (list (ossia-xml-fraction (ly:moment-main (ly:duration->moment d)))
           (ly:duration-log d) (ly:duration-dot-count d))))

#(define (ossia-xml-unsupported ctx e description)
   (ossia-xml-emit 'unsupported ctx e description))

#(define (ossia-xml-note e event)
   (let ((ctx (ly:translator-context e))
         (pitch (ly:event-property event 'pitch #f)))
     (if (ly:pitch? pitch)
         (ossia-xml-emit 'note ctx event (ossia-xml-pitch pitch) (ossia-xml-duration event)
          (and (not (zero? (ly:moment-grace (ly:context-current-moment ctx))))
               (equal? (assoc-get 'stroke-style (ly:context-grob-definition ctx 'Flag)) "grace"))
          (ly:event-property event 'force-accidental #f)
          (ly:event-property event 'cautionary #f)
          (map (lambda (art)
                 (list (car (ly:event-property art 'class))
                       (ly:event-property art 'span-direction 0)
                       (ly:event-property art 'articulation-type "")))
               (filter ly:stream-event? (ly:event-property event 'articulations '()))))
         (ossia-xml-unsupported ctx event "unpitched note"))))

#(define ((ossia-xml-rest kind) e event)
   (let ((ctx (ly:translator-context e)))
     (if (= 0 (ly:moment-grace-numerator (ly:context-current-moment ctx)))
         (ossia-xml-emit kind ctx event (ossia-xml-duration event))
         (ossia-xml-unsupported ctx event "grace rest"))))

#(define (ossia-xml-span e event)
   (let ((ctx (ly:translator-context e)))
     (ossia-xml-emit (car (ly:event-property event 'class)) ctx event
                     (ly:event-property event 'span-direction 0))))

#(define (ossia-xml-tuplet e event)
   (let ((ctx (ly:translator-context e)))
     (if (= START (ly:event-property event 'span-direction))
         (ossia-xml-emit 'tuplet ctx event
                         (ly:event-property event 'numerator)
                         (ly:event-property event 'denominator)
                         (ossia-xml-fraction (ly:moment-main (ly:event-property event 'length))))
         (ossia-xml-emit 'tuplet-end ctx event))))

#(define (ossia-xml-dynamic e event)
   (ossia-xml-emit 'dynamic (ly:translator-context e) event
                   (ossia-xml-text (ly:event-property event 'text))))

#(define ((ossia-xml-reject what) e event)
   (ossia-xml-unsupported (ly:translator-context e) event what))

#(define ossia-xml-stems (make-hash-table))
#(define ossia-xml-beams '())
#(define ossia-xml-tie-heads (make-hash-table))
#(define ossia-xml-ties '())

#(define (ossia-xml-write-ties)
   (for-each
    (lambda (tie)
      (let* ((left (ly:spanner-bound tie LEFT))
             (right (ly:spanner-bound tie RIGHT))
             (head (hashq-ref ossia-xml-tie-heads left))
             (next (hashq-ref ossia-xml-tie-heads right)))
        (when (and head next (= (car head) (car next)))
          (let* ((cause (ly:grob-property left 'cause))
                 (pitch (and (ly:stream-event? cause) (ly:event-property cause 'pitch #f))))
            (when (ly:pitch? pitch)
              (ossia-xml-line 'tied-head (car head) (cadr head)
                              (ossia-xml-origin cause) (list (ossia-xml-pitch pitch))))))))
    ossia-xml-ties))

#(define (ossia-xml-stem-counts stem)
   (let ((beaming (ly:grob-property stem 'beaming)))
     (if (pair? beaming)
         (map (lambda (side) (if (list? side) (length side) 0))
              (list (car beaming) (cdr beaming)))
         '(0 0))))

#(define (ossia-xml-write-beams)
   (for-each
    (lambda (beam)
      (let* ((array (ly:grob-object beam 'stems))
             (stems (if (ly:grob-array? array) (ly:grob-array->list array) '()))
             (anchors (map (lambda (stem) (hashq-ref ossia-xml-stems stem)) stems)))
        (when (and (pair? anchors) (every pair? anchors)
                   (every (lambda (anchor) (= (car anchor) (caar anchors))) anchors)
                   (any (lambda (anchor)
                          (or (not (= (caddr anchor) (caddar anchors)))
                              (not (equal? (cadr (cadr anchor)) '(0 1)))))
                        anchors))
          ;; Each side stores one entry per beam level. Compare adjacent stems
          ;; to distinguish connected beams from partial beams (hooks).
          (let loop ((anchors anchors) (counts (map ossia-xml-stem-counts stems)) (prev-right 0))
            (unless (null? anchors)
              (let* ((anchor (car anchors))
                     (left (caar counts))
                     (right (cadar counts))
                     (next-left (if (pair? (cdr counts)) (caadr counts) 0))
                     (levels
                      (map (lambda (level)
                             (let ((before (<= level (min left prev-right)))
                                   (after (<= level (min right next-left))))
                               (cond ((and before after) "continue")
                                     (before "end")
                                     (after "begin")
                                     ((<= level right) "forward hook")
                                     (else "backward hook"))))
                           (iota (max left right) 1))))
                (unless (null? levels)
                  (ossia-xml-line 'beam (car anchor) (cadr anchor) '() (list levels)))
                (loop (cdr anchors) (cdr counts) right)))))))
    ossia-xml-beams))

#(define (ossia-xml-voice ctx)
   (let ((staff 0) (anchor #f))
    (make-engraver
    (listeners
     ((note-event e event)
      (set! anchor event)
      (ossia-xml-note e event))
     ((rest-event e event)
      (set! anchor event)
      ((ossia-xml-rest 'rest) e event))
     ((multi-measure-rest-event e event)
      (set! anchor event)
      ((ossia-xml-rest 'measure-rest) e event))
     ((skip-event e event)
      (set! anchor event)
      ((ossia-xml-rest 'skip) e event))
     (tie-event . ossia-xml-span)
     (repeat-tie-event . ossia-xml-span)
     ((arpeggio-event e event)
      (let ((direction (assoc-get 'arpeggio-direction (ly:context-grob-definition ctx 'Arpeggio) 0)))
        (ossia-xml-emit 'arpeggio ctx event (if (integer? direction) direction 0))))
     (slur-event . ossia-xml-span)
     (phrasing-slur-event . ossia-xml-span)
     ((articulation-event e event)
      (ossia-xml-emit 'articulation-event (ly:translator-context e) event
                       (ly:event-property event 'articulation-type "")))
     (tuplet-span-event . ossia-xml-tuplet)
     (absolute-dynamic-event . ossia-xml-dynamic)
     (crescendo-event . ossia-xml-span)
     (decrescendo-event . ossia-xml-span)
     ((percent-event e event) ((ossia-xml-reject "percent repeat") e event))
     ((double-percent-event e event) ((ossia-xml-reject "percent repeat") e event))
     ((tremolo-event e event)
      (ossia-xml-emit 'tremolo-event (ly:translator-context e) event
                       (ly:event-property event 'tremolo-type)))
     ((tremolo-span-event e event) ((ossia-xml-reject "alternating tremolo")) e event))
    (acknowledgers
     ((note-head-interface e head source)
      (hashq-set! ossia-xml-tie-heads head
                  (list (ossia-xml-context ctx) (ossia-xml-moment ctx))))
     ((tie-interface e tie source)
      (set! ossia-xml-ties (cons tie ossia-xml-ties)))
     ((stem-interface e stem source)
      (hashq-set! ossia-xml-stems stem
                  (list (ossia-xml-context ctx) (ossia-xml-moment ctx)
                        (ossia-xml-context (ly:context-find ctx 'Staff))))))
    (end-acknowledgers
     ((beam-interface e beam source)
      (set! ossia-xml-beams (cons beam ossia-xml-beams))))
    ((initialize e)
     (ossia-xml-context ctx)
     (set! staff (ossia-xml-context (ly:context-find ctx 'Staff))))
    ((stop-translation-timestep e)
     ;; Reparenting changes the staff at this timestep but preserves the voice ID.
     (let ((current (ossia-xml-context (ly:context-find ctx 'Staff))))
       (unless (= staff current)
         (set! staff current)
         (ossia-xml-emit 'voice-staff ctx anchor current)))
     (set! anchor #f)))))

#(define (ossia-xml-lyric e event)
   (let* ((ctx (ly:translator-context e))
          (voice (ly:context-property ctx 'associatedVoiceContext #f)))
     (ossia-xml-emit (car (ly:event-property event 'class)) ctx event
                      (ossia-xml-context voice)
                      (ossia-xml-text (ly:event-property event 'text #f)))))

#(define ossia-xml-lyrics
   (make-engraver
    (listeners
     (lyric-event . ossia-xml-lyric)
     (hyphen-event . ossia-xml-lyric)
     (extender-event . ossia-xml-lyric))
    ((initialize e) (ossia-xml-context (ly:translator-context e)))))

#(define (ossia-xml-chords ctx)
   (let ((notes '()) (no-chord #f))
     (make-engraver
      (listeners
       ((note-event e event) (set! notes (cons event notes)))
       ((rest-event e event) (set! no-chord event)))
      ((initialize e) (ossia-xml-context ctx))
      ((stop-translation-timestep e)
       (when (or (pair? notes) no-chord)
         (let* ((properties (ly:context-grob-definition ctx 'ChordName))
                (visible (not (eq? #f (assoc-get 'stencil properties)))))
           (if (pair? notes)
               (let ((pitches
                      (filter-map
                       (lambda (n)
                         (let ((p (ly:event-property n 'pitch))
                               (oct (ly:event-property n 'octavation #f)))
                           (and (not (ly:event-property n 'bass #f))
                                (if (integer? oct)
                                    (ly:pitch-transpose p (ly:make-pitch (- oct) 0))
                                    p))))
                       notes))
                     (bass (find (lambda (n) (or (ly:event-property n 'bass #f)
                                                 (ly:event-property n 'inversion #f))) notes)))
                 (ossia-xml-emit 'harmony ctx (car notes)
                                  (map ossia-xml-pitch (sort pitches ly:pitch<?))
                                  (if bass (list (ossia-xml-pitch (ly:event-property bass 'pitch))) '())
                                  visible))
               (ossia-xml-emit 'no-chord ctx no-chord visible))))
       (set! notes '())
       (set! no-chord #f)))))

#(define (ossia-xml-key e event)
   (let* ((ctx (ly:translator-context e))
          (tonic (ly:event-property event 'tonic #f))
          (entries (ly:event-property event 'pitch-alist '())))
     (if (and (ly:pitch? tonic) (every (lambda (entry) (integer? (car entry))) entries))
         (ossia-xml-emit 'key ctx event (ossia-xml-pitch tonic)
                          (map (lambda (entry) (list (car entry) (ossia-xml-fraction (* 2 (cdr entry))))) entries))
         (ossia-xml-unsupported ctx event "nonstandard key signature"))))

#(define (ossia-xml-staff ctx)
   (let ((clef #f) (name #f) (transposition #f))
     (make-engraver
      (listeners (key-change-event . ossia-xml-key))
      ((initialize e) (ossia-xml-context ctx))
      ((stop-translation-timestep e)
       (let ((score (ly:context-find ctx 'Score)))
         (when (and score
                    (any (lambda (key) (not (equal? (ly:context-property score key)
                                                    (ly:context-property ctx key))))
                         '(measurePosition measureLength timeSignature)))
           (ossia-xml-unsupported ctx #f "independent staff meters")))
       (let ((spec (list (ly:context-property ctx 'clefGlyph "")
                         (ly:context-property ctx 'clefPosition 0)
                         (ly:context-property ctx 'clefTransposition 0)))
             (label (ossia-xml-text (ly:context-property ctx 'instrumentName "")))
             (pitch (ly:context-property ctx 'instrumentTransposition #f)))
         (when (and (ly:pitch? pitch) (not (equal? pitch transposition)))
           (set! transposition pitch)
           (unless (and (= (ly:pitch-steps pitch) 0) (= (ly:pitch-semitones pitch) 0)
                        (= (ossia-xml-time ctx) 0))
             (ossia-xml-emit 'transpose ctx #f (ly:pitch-steps pitch)
                             (ossia-xml-fraction (ly:pitch-semitones pitch)))))
         (unless (equal? spec clef)
           (set! clef spec)
           (apply ossia-xml-emit 'clef ctx #f spec))
         (unless (equal? name label)
           (set! name label)
           (ossia-xml-emit 'name ctx #f label)))))))

#(define (ossia-xml-group ctx)
   (make-engraver
    ((initialize e) (ossia-xml-context ctx))
    ((finalize e)
     (ossia-xml-emit 'name ctx #f
                      (ossia-xml-text (ly:context-property ctx 'instrumentName ""))))))

#(define (ossia-xml-tempo e event)
   (let ((ctx (ly:translator-context e))
         (unit (ly:event-property event 'tempo-unit #f))
         (count (ly:event-property event 'metronome-count #f))
         (text (ossia-xml-text (ly:event-property event 'text #f))))
     (cond
      ((and (ly:duration? unit) (number? count))
       (let ((speed (inexact->exact count)))
         (ossia-xml-emit 'tempo ctx event text
          (list (list (ly:duration-log unit) (ly:duration-dot-count unit)
                      (ossia-xml-fraction speed)
                      (ossia-xml-fraction (* speed 4 (ly:moment-main (ly:duration->moment unit)))))))))
      ((ly:duration? unit) (ossia-xml-unsupported ctx event "metronome ranges"))
      ((not (string-null? text)) (ossia-xml-emit 'tempo ctx event text '())))))

#(define (ossia-xml-repeat e event)
   (ossia-xml-emit (car (ly:event-property event 'class)) (ly:translator-context e) event
                   (ly:event-property event 'repeat-count 2)
                   (ly:event-property event 'volta-numbers '())
                   (ly:event-property event 'span-direction 0)))

#(define (ossia-xml-discover-contexts ctx)
   (ossia-xml-context ctx)
   (for-each ossia-xml-discover-contexts (ly:context-children ctx)))

#(define (ossia-xml-score ctx)
   (let ((grid #f))
     (make-engraver
      (listeners
       (tempo-change-event . ossia-xml-tempo)
       (volta-repeat-start-event . ossia-xml-repeat)
       (volta-repeat-end-event . ossia-xml-repeat)
       (volta-span-event . ossia-xml-repeat)
       ((fine-event e event) ((ossia-xml-reject "Fine jump") e event))
       ((dal-segno-event e event) ((ossia-xml-reject "D.S. or D.C. jump") e event))
       ((segno-mark-event e event) ((ossia-xml-reject "segno jump") e event))
       ((coda-mark-event e event) ((ossia-xml-reject "coda jump") e event))
       ((ad-hoc-jump-event e event) ((ossia-xml-reject "custom repeat jump")) e event))
      ((initialize e) (ossia-xml-context ctx))
      ((stop-translation-timestep e)
       (ossia-xml-discover-contexts ctx)
       (let* ((pos (ly:moment-main (ly:context-property ctx 'measurePosition (ly:make-moment 0))))
              (len (ly:context-property ctx 'measureLength 1))
              (meter (ly:context-property ctx 'timeSignature '(4 . 4)))
              (start (- (ossia-xml-time ctx) pos))
              (spec (list start len meter)))
         (if (or (not (ly:context-property ctx 'timing #t))
                 (not (rational? len)) (not (pair? meter)))
             (ossia-xml-unsupported ctx #f "unmetered or unsupported time signature")
             (unless (equal? spec grid)
               (set! grid spec)
               (ossia-xml-emit 'grid ctx #f (ossia-xml-fraction pos) (ossia-xml-fraction len)
                               (car meter) (cdr meter))))))
      ((finalize e) (ossia-xml-emit 'end ctx #f)))))

#(define (ossia-xml-header key score book)
   (or (any (lambda (scope)
              (and (module? scope)
                   (let ((var (module-variable scope key)))
                     (and var (variable-bound? var) (ossia-xml-text (variable-ref var))))))
            (list (ly:score-header score) (ly:book-header book) (ly:parser-lookup '$defaultheader)))
       ""))

#(set-object-property! 'ossia-xml-after-grace 'music-type? boolean?)
#(define ossia-xml-original-after-grace
   (ly:music-function-extract (ly:parser-lookup 'afterGrace)))
afterGrace =
#(define-music-function (fraction main grace) ((scale?) ly:music? ly:music?)
   (let ((music (ossia-xml-original-after-grace fraction main grace)))
     (ly:music-set-property! music 'ossia-xml-after-grace #t)
     music))

#(define (ossia-xml-unsupported-music music in-volta?)
   (and (ly:music? music)
        (let* ((name (ly:music-property music 'name))
               (volta? (eq? name 'VoltaRepeatedMusic))
               (what (cond
                      ((ly:music-property music 'ossia-xml-after-grace #f) "after-grace notes")
                      ((eq? name 'PartCombineMusic) "partCombine")
                      ((eq? name 'PercentRepeatedMusic) "percent repeats")
                      ((eq? name 'SegnoRepeatedMusic) "segno jumps")
                      ((and volta? in-volta?) "nested volta repeats")
                      (else #f))))
          (if what
              (cons music what)
              (let ((walk (lambda (item) (ossia-xml-unsupported-music item (or in-volta? volta?)))))
                (or (walk (ly:music-property music 'element #f))
                    (any walk (ly:music-property music 'elements '()))))))))

#(define (ossia-xml-kind d) (ly:output-def-lookup d 'output-def-kind))
#(define (ossia-xml-book book)
   (set! ossia-xml-books (+ ossia-xml-books 1))
   (let* ((scores (filter ly:score? (ly:book-scores book)))
          (layouts (remove
                    (lambda (score)
                      (let ((defs (ly:score-output-defs score)))
                        (and (pair? defs) (every (lambda (d) (eq? (ossia-xml-kind d) 'midi)) defs))))
                    scores)))
     (if (or (> ossia-xml-books 1) (pair? (ly:book-book-parts book)) (> (length scores) 2)
             (not (= (length layouts) 1)))
         (ossia-xml-line 'reject "MusicXML requires one unambiguous layout score")
         (let* ((score (car layouts))
                (layout (ly:output-def-clone
                         (or (find (lambda (d) (eq? (ossia-xml-kind d) 'layout))
                                   (ly:score-output-defs score))
                             (ly:parser-lookup '$defaultlayout))))
                (paper (ly:parser-lookup '$defaultpaper))
                (music (ly:score-music score))
                (unsupported (ossia-xml-unsupported-music music #f)))
           (when unsupported
             (ossia-xml-line 'reject (string-append "MusicXML export does not support " (cdr unsupported))
                             (ossia-xml-origin (car unsupported))))
           (ossia-xml-line 'metadata
                           (ossia-xml-header 'title score book)
                           (ossia-xml-header 'composer score book)
                           (ossia-xml-header 'poet score book))
           ;; The translation API does not attach the book paper scope to the layout like printing does.
           (module-for-each
            (lambda (sym var)
              (when (and (variable-bound? var) (eq? (ly:output-def-lookup layout sym 'unset) 'unset))
                (ly:output-def-set-variable! layout sym (variable-ref var))))
            (ly:output-def-scope paper))
           (unless unsupported
             (ly:run-translator music layout)
             (ossia-xml-write-beams)
             (ossia-xml-write-ties))))
     (force-output ossia-xml-port)))

#(define toplevel-book-handler ossia-xml-book)
#(define default-toplevel-book-handler ossia-xml-book)

\include "tablature.ily"

\layout {
  \context { \Score \consists #ossia-xml-score }
  \context { \Voice \consists #ossia-xml-voice }
  \context { \Staff \consists #ossia-xml-staff }
  \context { \PianoStaff \consists #ossia-xml-group }
  \context { \GrandStaff \consists #ossia-xml-group }
  \context { \StaffGroup \consists #ossia-xml-group }
  \context { \ChoirStaff \consists #ossia-xml-group }
  \context { \Lyrics \consists #ossia-xml-lyrics }
  \context { \ChordNames \consists #ossia-xml-chords }
  \context { \TabStaff \consists #ossia-xml-staff \consists #ossia-xml-tab-staff }
  \context { \TabVoice \consists #ossia-xml-voice \consists #ossia-xml-tab-voice }
  \context { \FretBoards \consists #ossia-xml-chords \consists #ossia-xml-fretboards }
  \context { \DrumStaff \consists #ossia-xml-group }
  \context { \RhythmicStaff \consists #ossia-xml-group }
  \context { \Dynamics \consists #ossia-xml-voice }
}
