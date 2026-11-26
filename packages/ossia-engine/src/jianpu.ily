#(define ossia-jianpu-staves (make-hash-table))
#(define ossia-jianpu-major '(0 2 4 5 7 9 11))
#(define ossia-jianpu-labels '("C" "D" "E" "F" "G" "A" "B"))

#(define (ossia-jianpu-reject event feature)
   (let ((origin (and (ly:stream-event? event) (ly:event-property event 'origin #f))))
     (if (ly:input-location? origin)
         (ly:input-warning origin "Ossia Jianpu: ~a" feature)
         (ly:warning "Ossia Jianpu: ~a" feature))))

#(define (ossia-jianpu-accidental alteration)
   (case alteration
     ((-2) "accidentals.flatflat") ((-1) "accidentals.flat")
     ((1) "accidentals.sharp") ((2) "accidentals.doublesharp")
     (else #f)))

#(define (ossia-jianpu-key event)
   (let* ((tonic (ly:event-property event 'tonic #f))
          (entries (ly:event-property event 'pitch-alist '())))
     (and (ly:pitch? tonic)
          (every (lambda (entry)
                   (and (integer? (car entry)) (memv (cdr entry) '(-1/2 0 1/2))))
                 entries)
          (let* ((fifths (* 2 (apply + (map cdr entries))))
                 (major (modulo (* 4 fifths) 7))
                 (major-semitone (+ (list-ref ossia-jianpu-major major)
                                    (* 2 (or (assv-ref entries major) 0))))
                 (name (ly:pitch-notename tonic))
                 (degree (modulo (- name major) 7))
                 (alteration (* 2 (ly:pitch-alteration tonic))))
            (and (<= -7 fifths 7)
                 (memv degree '(0 5))
                 (= alteration (* 2 (or (assv-ref entries name) 0)))
                 (every
                   (lambda (step)
                     (= (+ (list-ref ossia-jianpu-major step)
                           (* 2 (or (assv-ref entries step) 0))
                           (if (< step major) 12 0))
                        (+ major-semitone (list-ref ossia-jianpu-major (modulo (- step major) 7)))))
                   (iota 7))
                 (list major major-semitone (if (= degree 0) "1 = " "6 = ")
                       (list-ref ossia-jianpu-labels name) alteration))))))

#(define ossia-jianpu-default-key '(0 0 "1 = " "C" 0))

#(define (ossia-jianpu-dots count)
   (make-concat-markup (make-list count (make-simple-markup "."))))

#(define (ossia-jianpu-number grob number alteration octave underlines dots)
   (let* ((text (grob-interpret-markup grob
                  (make-fontsize-markup 0
                    (make-bold-markup (make-simple-markup number)))))
          (digit (ly:stencil-aligned-to
                   (ly:stencil-aligned-to text X CENTER) Y CENTER))
          (x (ly:stencil-extent digit X))
          (y (ly:stencil-extent digit Y))
          (accidental (ossia-jianpu-accidental alteration))
          (lines (map (lambda (i)
                        (let ((at (- (car y) 0.24 (* 0.27 i))))
                          (make-line-stencil 0.11 (car x) at (cdr x) at)))
                      (iota underlines)))
          (points (map (lambda (i)
                         (let ((at (if (> octave 0)
                                       (+ (cdr y) 0.27 (* 0.29 i))
                                       (- (car y) 0.28 (* 0.27 underlines) (* 0.29 i)))))
                           (ly:stencil-translate-axis (make-circle-stencil 0.095 0.02 #t) at Y)))
                       (iota (abs octave))))
          (with-alteration
            (if accidental
                (ly:stencil-combine-at-edge digit X LEFT
                  (ly:stencil-aligned-to
                    (grob-interpret-markup grob
                      (make-fontsize-markup -3 (make-musicglyph-markup accidental)))
                    Y CENTER)
                  0.12)
                digit))
          (with-dots
            (if (> dots 0)
                (ly:stencil-combine-at-edge with-alteration X RIGHT
                  (grob-interpret-markup grob (ossia-jianpu-dots dots)) 0.10)
                with-alteration)))
     (let* ((shape (apply ly:stencil-add (append (list with-dots) lines points)))
            (ink (ly:stencil-extent shape X))
            (half-width (max (abs (car ink)) (abs (cdr ink)))))
       (ly:make-stencil (ly:stencil-expr shape)
         (cons (- half-width) half-width) (ly:stencil-extent shape Y)))))

#(define (ossia-jianpu-degree pitch key)
   (let* ((major (car key))
          (step (ly:pitch-notename pitch))
          (degree (modulo (- step major) 7))
          (wrap (if (< step major) 1 0))
          (octave (- (ly:pitch-octave pitch) wrap))
          (alteration (- (+ (list-ref ossia-jianpu-major step)
                            (* 2 (ly:pitch-alteration pitch)) (* wrap 12))
                         (cadr key) (list-ref ossia-jianpu-major degree))))
     (list degree alteration octave)))

#(define (ossia-jianpu-rhythm-stencil grob digit alteration octave duration grace? slashed?)
   (let ((log (ly:duration-log duration)))
     (let ((stencil
             (ossia-jianpu-number grob digit alteration octave
               (max 0 (- log 2))
               (if (or grace? (>= log 2)) (ly:duration-dot-count duration) 0))))
       (if grace?
           (ly:stencil-scale
             (if slashed?
                 (ly:stencil-add stencil (make-line-stencil 0.13 -0.7 -0.45 0.7 0.4))
                 stencil)
             0.65 0.65)
           stencil))))

#(define (ossia-jianpu-ink x y)
   (if ossia-jianpu-svg
       (ly:make-stencil `(url-link "ossia:JianpuStaff:-1:-1:-1" ,x ,y) x y)
       (ly:make-stencil '() x y)))

#(define (ossia-jianpu-lane-offset staff lane)
   ;; Lane extents accumulate during translation. Position callbacks use the final spacing.
   (let loop ((lanes (vector-ref staff 1)) (offset 0))
     (let ((current (car lanes)))
       (if (eq? lane current)
           offset
           (loop (cdr lanes)
                 (- offset (max 6.5 (+ 3 (- (vector-ref current 1))
                                        (vector-ref (cadr lanes) 2)))))))))

#(define (ossia-jianpu-staff-extent data)
   (let loop ((lanes (vector-ref data 1)) (low 0) (high 0))
     (if (null? lanes)
         (cons low high)
         (let* ((lane (car lanes))
                (offset (ossia-jianpu-lane-offset data lane)))
           (loop (cdr lanes)
                 (min low (+ offset (vector-ref lane 1)))
                 (max high (+ offset (vector-ref lane 2))))))))

#(define (ossia-jianpu-bar-extent data)
   (let ((lanes (vector-ref data 1)))
     (cons (if (null? lanes) -1.4
               (min -1.4 (- (ossia-jianpu-lane-offset data (last lanes)) 1.1)))
           1.4)))

#(define (ossia-jianpu-staff context)
   (let ((data (vector #f '() ossia-jianpu-default-key #f)))
     (hashq-set! ossia-jianpu-staves context data)
     (make-engraver
       (listeners
         ((key-change-event engraver event)
          (let ((key (ossia-jianpu-key event)))
            (cond ((not key) (ossia-jianpu-reject event "a key outside standard major/minor"))
                  ((and (or (vector-ref data 3)
                            (> (ly:moment-main (ly:context-current-moment context)) 0))
                        (not (equal? key (vector-ref data 2))))
                   (ossia-jianpu-reject event "a key change"))
                  (else
                    (vector-set! data 2 key)
                    (vector-set! data 3 #t))))))
       (acknowledgers
         ((staff-symbol-interface engraver grob source)
          (vector-set! data 0 grob)
          (ly:grob-set-property! grob 'stencil
            (lambda (piece)
              (let* ((original (ly:staff-symbol::print piece))
                     (x (ly:stencil-extent original X))
                     (lanes (ossia-jianpu-staff-extent data)))
                (ossia-jianpu-ink x (cons (- (car lanes) 2) (+ (cdr lanes) 2)))))))
         ((bar-line-interface engraver grob source)
          (ly:grob-set-property! grob 'bar-extent
            (lambda (piece) (ossia-jianpu-bar-extent data))))
         ((time-signature-interface engraver grob source)
          (let ((first? (zero? (ly:moment-main (ly:context-current-moment context)))))
            (when first?
              (ly:grob-set-property! grob 'stencil
                (lambda (piece)
                  (let* ((key (vector-ref data 2))
                         (accidental (ossia-jianpu-accidental (list-ref key 4)))
                         (label (make-concat-markup
                                  (append (list (make-simple-markup (list-ref key 2))
                                                (make-simple-markup (list-ref key 3)))
                                          (if accidental (list (make-musicglyph-markup accidental)) '())))))
                    (ly:stencil-combine-at-edge
                      (ly:time-signature::print piece) X LEFT
                      (ly:stencil-aligned-to (grob-interpret-markup piece label) Y CENTER)
                      0.7)))))))))))

#(define (ossia-jianpu-voice context)
   (let ((heads '()) (rest #f) (pending '()) (staff #f) (lane #f)
         (home (ly:context-find context 'Staff)) (tuplet-scale #f))
     (define (register! event)
       (let* ((target (ly:context-find context 'Staff))
              (data (hashq-ref ossia-jianpu-staves target)))
         (when (and target home (not (eq? target home))
                    (let ((parent (ly:context-parent target)))
                      (not (and (eq? parent (ly:context-parent home))
                                (eq? (ly:context-name parent) 'PianoStaff)))))
           (ossia-jianpu-reject event "a staff change outside the same PianoStaff"))
         (when (and data (not (eq? data staff)))
           (set! staff data)
           (let* ((lanes (vector-ref staff 1))
                  (known (find (lambda (candidate) (eq? context (vector-ref candidate 0))) lanes)))
             (set! lane (or known (vector context 0 0)))
             (unless known
               (vector-set! staff 1 (append lanes (list lane))))))))
     (define (grace?) (not (zero? (ly:moment-grace (ly:context-current-moment context)))))
     (define (event-duration engraver event)
       (register! event)
       (let* ((now (ly:context-current-moment context))
              (duration (ly:event-property event 'duration))
              (scale (ly:duration-scale duration))
              (log (ly:duration-log duration)))
         (cond
           ((or (not (= scale (or tuplet-scale 1)))
                (< log 0)
                (> (ly:duration-dot-count duration) 3))
            (ossia-jianpu-reject event "a scaled or uncommon written duration"))
           ((and (< log 2) (not (grace?)))
            (let* ((length (/ (ly:moment-main (ly:duration->moment duration)) scale))
                   (start (ly:moment-main now))
                   (rest? (ly:in-event-class? event 'rest-event))
                   (tail (modulo (* 32 length) 8))
                   (steps (iota (1- (ceiling (* 4 length))) 1)))
              (for-each
                (lambda (step)
                  (let ((at (+ start (* scale (/ step 4)))))
                    (unless (assv at pending)
                      (set! pending
                        (acons at (list staff lane rest?
                                        (if (= step (last steps))
                                            (case tail ((4) 1) ((6) 2) ((7) 3) (else 0))
                                            0)) pending))
                      (ly:context-schedule-moment context (ly:make-moment at)))))
                steps))))))
     (make-engraver
       (listeners
         ((tuplet-span-event engraver event)
          (if (= (ly:event-property event 'span-direction) START)
              (if tuplet-scale
                  (ossia-jianpu-reject event "nested tuplets")
                  (set! tuplet-scale (/ (ly:event-property event 'numerator)
                                        (ly:event-property event 'denominator))))
              (set! tuplet-scale #f)))
         (note-event . event-duration)
         (rest-event . event-duration)
         ((multi-measure-rest-event engraver event)
          (ossia-jianpu-reject event "multi-measure rests")))
       ((process-music engraver)
        (let* ((now (ly:moment-main (ly:context-current-moment context)))
               (next (and (not (grace?)) (assv now pending))))
          (when next
            (let* ((grob (ly:engraver-make-grob engraver 'NoteName '()))
                   (spec (cdr next))
                   (start-staff (list-ref spec 0))
                   (start-lane (list-ref spec 1)))
              (ly:grob-set-parent! grob X (ly:context-property context 'currentMusicalColumn))
              (ly:grob-set-parent! grob Y (vector-ref start-staff 0))
              (ly:grob-set-property! grob 'Y-offset
                (lambda (piece) (ossia-jianpu-lane-offset start-staff start-lane)))
              (ly:grob-set-property! grob 'text
                (make-concat-markup
                  (list (make-simple-markup (if (list-ref spec 2) "0" "–"))
                        (ossia-jianpu-dots (list-ref spec 3))))))
            (set! pending (assv-remove! pending now)))))
       (acknowledgers
         ((note-head-interface engraver grob source)
          (let* ((event (ly:grob-property grob 'cause))
                 (pitch (ly:event-property event 'pitch #f)))
            (when (and staff (ly:pitch? pitch))
              (apply (lambda (degree alteration octave)
                       (let* ((duration (ly:event-property event 'duration))
                              (grace (grace?))
                              (slashed (and grace (equal? "grace"
                                         (assq-ref (ly:context-grob-definition context 'Flag)
                                                   'stroke-style)))))
                         (unless (memv alteration '(-2 -1 0 1 2))
                           (ossia-jianpu-reject event "a microtonal or unusually spelled pitch"))
                         (ly:grob-set-property! grob 'stencil
                           (lambda (head)
                             (ossia-jianpu-rhythm-stencil head (number->string (1+ degree))
                               alteration octave duration grace slashed)))
                         (set! heads (cons (cons pitch grob) heads))))
                     (ossia-jianpu-degree pitch (vector-ref staff 2))))))
         ((rest-interface engraver grob source)
          (let* ((event (ly:grob-property grob 'cause))
                 (duration (ly:event-property event 'duration))
                 (grace (grace?)))
            (ly:grob-set-property! grob 'stencil
              (lambda (piece) (ossia-jianpu-rhythm-stencil piece "0" 0 0 duration grace #f)))
            (set! rest grob))))
       ((process-acknowledged engraver)
        (when (and staff lane)
          (let* ((target staff)
                 (voice-lane lane)
                 (count (length heads))
                 (step (if (grace?) 1.6 2.5))
                 (height (* step (max 0 (1- count))))
                 (top (if (grace?) 1.35 0))
                 (base (if (eq? lane (car (vector-ref staff 1))) top (- top height))))
            (vector-set! lane 1 (min (vector-ref lane 1) base))
            (vector-set! lane 2 (max (vector-ref lane 2) (+ base height)))
            (for-each
              (lambda (head i)
                (let ((position (+ base (* step i))))
                  (ly:grob-set-property! (cdr head) 'staff-position
                    (lambda (piece)
                      (* 2 (+ position (ossia-jianpu-lane-offset target voice-lane)))))))
              (sort heads (lambda (a b) (ly:pitch<? (car a) (car b))))
              (iota count))
            (when rest
              (ly:grob-set-property! rest 'Y-offset
                (lambda (piece) (+ top (ossia-jianpu-lane-offset target voice-lane))))))))
       ((stop-translation-timestep engraver)
        (set! heads '())
        (set! rest #f)))))

#(define (ossia-jianpu-score context)
   (make-engraver
     (acknowledgers
       ((staff-symbol-interface engraver grob source)
        (unless (eq? (ly:context-name (ly:translator-context source)) 'Staff)
          (ossia-jianpu-reject #f "a staff other than a pitched Staff context"))))))

\paper {
  markup-system-spacing.padding = #2
}

\layout {
  \context {
    \Score
    \consists #ossia-jianpu-score
  }
  \context {
    \Staff
    \consists #ossia-jianpu-staff
    \remove Ledger_line_engraver
    \remove Note_collision_engraver
    \remove Rest_collision_engraver
    \override Clef.stencil = ##f
    \override OttavaBracket.stencil = ##f
    \override KeySignature.stencil = ##f
    \override KeyCancellation.stencil = ##f
    \override Accidental.stencil = ##f
    \override AccidentalCautionary.stencil = ##f
    \override TimeSignature.style = #'numbered
  }
  \context {
    \Voice
    \consists #ossia-jianpu-voice
    \override NoteHead.X-offset = #0
    \override NoteHead.extra-spacing-height = #'(0 . 0)
    \override NoteHead.stem-attachment = #'(0 . 0)
    \override Rest.X-extent = #ly:grob::stencil-width
    \override Rest.Y-extent = #grob::always-Y-extent-from-stencil
    \override Stem.transparent = ##t
    \override Beam.transparent = ##t
    \override Flag.transparent = ##t
    \override Dots.transparent = ##t
    \override TupletBracket.bracket-visibility = ##t
    \override TupletNumber.text =
      #(lambda (grob)
         (let ((cause (event-cause grob)))
           (if (and (= (ly:event-property cause 'numerator) 2)
                    (= (ly:event-property cause 'denominator) 3))
               (tuplet-number::calc-denominator-text grob)
               (tuplet-number::calc-fraction-text grob))))
  }
}
