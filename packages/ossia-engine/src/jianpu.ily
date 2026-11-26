#(define ossia-jianpu-staves (make-hash-table))
#(define ossia-jianpu-major '(0 2 4 5 7 9 11))
#(define ossia-jianpu-labels '("C" "D" "E" "F" "G" "A" "B"))

#(define (ossia-jianpu-reject event feature)
   (let ((origin (and (ly:stream-event? event) (ly:event-property event 'origin #f))))
     (if (ly:input-location? origin)
         (ly:input-warning origin "Ossia Jianpu: ~a" feature)
         (ly:warning "Ossia Jianpu: ~a" feature))))

#(define (ossia-jianpu-staff-data context)
   (let ((staff (ly:context-find context 'Staff)))
     (hashq-ref ossia-jianpu-staves staff)))

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

#(define (ossia-jianpu-pitch-stencil grob degree alteration octave duration)
   (let ((log (ly:duration-log duration)))
     (ossia-jianpu-number grob (number->string (1+ degree)) alteration octave
                          (max 0 (- log 2))
                          (if (>= log 2) (ly:duration-dot-count duration) 0))))

#(define (ossia-jianpu-rest-stencil grob duration)
   (let ((log (ly:duration-log duration)))
     (ossia-jianpu-number grob "0" 0 0 (max 0 (- log 2))
                          (if (>= log 2) (ly:duration-dot-count duration) 0))))

#(define (ossia-jianpu-ink x y)
   (if ossia-jianpu-svg
       (ly:make-stencil `(url-link "ossia:JianpuStaff:-1:-1:-1" ,x ,y) x y)
       (ly:make-stencil '() x y)))

#(define (ossia-jianpu-bar-extent data)
   (cons (if (> (length (vector-ref data 1)) 1) -7.6 -1.4) 1.4))

#(define (ossia-jianpu-staff context)
   ;; The last pair tracks the lowest and highest numeric glyph positions in staff spaces.
   (let ((data (vector #f '() ossia-jianpu-default-key #f (cons 0 0))))
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
                     (lanes (vector-ref data 4)))
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
   (let ((heads '()) (rest #f) (pending '()) (staff #f) (lane #f))
     (define (register! event)
       (let ((data (ossia-jianpu-staff-data context)))
         (cond
           ((and staff (not (eq? staff data))) (ossia-jianpu-reject event "a cross-staff voice"))
           ((and data (not staff))
            (set! staff data)
            (let ((voices (vector-ref staff 1)))
              (set! lane (length voices))
              (when (> lane 1)
                (ossia-jianpu-reject event "more than two voices on one staff"))
              (vector-set! staff 1 (append voices (list context))))))))
     (define (offset) (* -6.5 (or lane 0)))
     (define (event-duration engraver event)
       (register! event)
       (let* ((now (ly:context-current-moment context))
              (duration (ly:event-property event 'duration))
              (log (ly:duration-log duration)))
         (cond
           ((not (zero? (ly:moment-grace now))) (ossia-jianpu-reject event "grace notes"))
           ((or (not (= (ly:duration-scale duration) 1))
                (< log 0)
                (> (ly:duration-dot-count duration) 3))
            (ossia-jianpu-reject event "a scaled, tuplet or uncommon written duration"))
           ((< log 2)
            (let* ((length (ly:moment-main (ly:duration->moment duration)))
                   (start (ly:moment-main now))
                   (rest? (ly:in-event-class? event 'rest-event))
                   (tail (modulo (* 32 length) 8))
                   (steps (iota (1- (ceiling (* 4 length))) 1)))
              (for-each
                (lambda (step)
                  (let ((at (+ start (/ step 4))))
                    (unless (assv at pending)
                      (set! pending
                        (acons at (cons rest? (if (= step (last steps))
                                                  (case tail ((4) 1) ((6) 2) ((7) 3) (else 0))
                                                  0)) pending))
                      (ly:context-schedule-moment context (ly:make-moment at)))))
                steps))))))
     (make-engraver
       (listeners
         (note-event . event-duration)
         (rest-event . event-duration)
         ((multi-measure-rest-event engraver event)
          (ossia-jianpu-reject event "multi-measure rests")))
       ((process-music engraver)
        (let* ((now (ly:moment-main (ly:context-current-moment context)))
               (next (assv now pending)))
          (when (and next staff)
            (let ((grob (ly:engraver-make-grob engraver 'NoteName '()))
                  (spec (cdr next)))
              (ly:grob-set-parent! grob X (ly:context-property context 'currentMusicalColumn))
              (ly:grob-set-parent! grob Y (vector-ref staff 0))
              (ly:grob-set-property! grob 'Y-offset (offset))
              (ly:grob-set-property! grob 'text
                (make-concat-markup
                  (list (make-simple-markup (if (car spec) "0" "–"))
                        (ossia-jianpu-dots (cdr spec)))))))
          (when next (set! pending (assv-remove! pending now)))))
       (acknowledgers
         ((note-head-interface engraver grob source)
          (let* ((event (ly:grob-property grob 'cause))
                 (pitch (ly:event-property event 'pitch #f)))
            (when (and staff (ly:pitch? pitch))
              (apply (lambda (degree alteration octave)
                       (let ((duration (ly:event-property event 'duration)))
                         (unless (memv alteration '(-2 -1 0 1 2))
                           (ossia-jianpu-reject event "a microtonal or unusually spelled pitch"))
                         (ly:grob-set-property! grob 'stencil
                           (lambda (head) (ossia-jianpu-pitch-stencil head degree alteration octave duration)))
                         (set! heads (cons (cons pitch grob) heads))))
                     (ossia-jianpu-degree pitch (vector-ref staff 2))))))
         ((rest-interface engraver grob source)
          (let* ((event (ly:grob-property grob 'cause))
                 (duration (ly:event-property event 'duration)))
            (ly:grob-set-property! grob 'stencil (lambda (piece) (ossia-jianpu-rest-stencil piece duration)))
            (set! rest grob))))
       ((process-acknowledged engraver)
        (when (and staff (or (pair? heads) rest))
          (let* ((count (length heads))
                 (base (if (zero? lane) (offset) (- (offset) (* 2.5 (max 0 (1- count))))))
                 (extents (vector-ref staff 4)))
            (set-car! extents (min (car extents) base))
            (set-cdr! extents (max (cdr extents) (+ base (* 2.5 (max 0 (1- count))))))
            (for-each
              (lambda (head i)
                (ly:grob-set-property! (cdr head) 'staff-position (+ (* 2 base) (* 5 i))))
              (sort heads (lambda (a b) (ly:pitch<? (car a) (car b))))
              (iota count))))
        (when rest (ly:grob-set-property! rest 'Y-offset (offset))))
       ((stop-translation-timestep engraver)
        (set! heads '())
        (set! rest #f)))))

#(define (ossia-jianpu-score context)
   (let ((staves '()))
     (make-engraver
       (acknowledgers
         ((staff-symbol-interface engraver grob source)
          (let ((staff (ly:translator-context source)))
            (unless (memq staff staves)
              (set! staves (cons staff staves))
              (unless (and (eq? (ly:context-name staff) 'Staff)
                           (or (= (length staves) 1)
                               (and (= (length staves) 2)
                                    (eq? (ly:context-parent staff) (ly:context-parent (cadr staves)))
                                    (eq? (ly:context-name (ly:context-parent staff)) 'PianoStaff))))
                (ossia-jianpu-reject #f "a layout other than one pitched staff or two PianoStaff staves")))))))))

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
  }
}
