% Interactive geometry is emitted only into derived SVG sources.
#(define ossia-origins (make-hash-table))
#(define (ossia-set-origin kind source)
   (lambda (context)
     (let* ((staff (or (ly:context-find context 'Staff) context))
            (origins (hashq-ref ossia-origins staff '())))
       (hashq-set! ossia-origins staff (acons kind source origins)))))

#(define (ossia-origin grob)
   (let loop ((cause (ly:grob-property grob 'cause)) (depth 0))
     (cond ((> depth 8) #f)
           ((ly:stream-event? cause) (ly:event-property cause 'origin #f))
           ((ly:grob? cause) (loop (ly:grob-property cause 'cause) (1+ depth)))
           (else #f))))

#(define (ossia-curve-point controls t)
   (let* ((u (- 1 t))
          (weights (list (* u u u) (* 3 u u t) (* 3 u t t) (* t t t))))
     (cons (apply + (map (lambda (p w) (* (car p) w)) controls weights))
           (apply + (map (lambda (p w) (* (cdr p) w)) controls weights)))))

#(define (ossia-hit-stencils grob x y url)
   (let* ((kind (grob::name grob))
          (controls (ly:grob-property grob 'control-points '()))
          (steps (max 2 (min 512 (inexact->exact (ceiling (/ (- (cdr x) (car x)) 0.35))))))
          (paths
            (cond
              ((and (memq kind '(Slur PhrasingSlur Tie)) (list? controls) (= (length controls) 4))
               (list (map (lambda (i) (ossia-curve-point controls (/ i steps))) (iota (1+ steps)))))
              ((eq? kind 'Hairpin)
               (let* ((middle (/ (+ (car y) (cdr y)) 2))
                      (cresc (= (ly:grob-property grob 'grow-direction RIGHT) RIGHT))
                      (point (cons (if cresc (car x) (cdr x)) middle))
                      (edge (if cresc (cdr x) (car x))))
                 (map (lambda (height)
                        (map (lambda (i)
                               (let ((t (/ i steps)))
                                 (cons (+ (* (- 1 t) (car point)) (* t edge))
                                       (+ (* (- 1 t) middle) (* t height)))))
                             (iota (1+ steps))))
                      (list (car y) (cdr y)))))
              (else '())))
          (hit-url (string-append "ossia-hit:" (substring url 6))))
     (append-map
       (lambda (points)
         (map (lambda (a b)
                (let ((bx (cons (- (min (car a) (car b)) 0.2) (+ (max (car a) (car b)) 0.2)))
                      (by (cons (- (min (cdr a) (cdr b)) 0.2) (+ (max (cdr a) (cdr b)) 0.2))))
                  (ly:make-stencil `(url-link ,hit-url ,bx ,by) bx by)))
              (drop-right points 1) (cdr points)))
       paths)))

#(define (ossia-box stil url . context)
   (if (ly:stencil? stil)
       (let ((x (ly:stencil-extent stil X)) (y (ly:stencil-extent stil Y)))
         (if (and (< (car x) (cdr x)) (< (car y) (cdr y)))
             (let* ((hits (if (pair? context) (ossia-hit-stencils (car context) x y url) '()))
                    (combined (apply ly:stencil-add (cons (ly:make-stencil `(url-link ,url ,x ,y) x y) (cons stil hits)))))
               (ly:make-stencil (ly:stencil-expr combined) x y))
             stil))
       stil))

#(define-markup-command (ossiaHeader layout props kind source arg)
   (string? string? markup?)
   (ossia-box (interpret-markup layout props arg)
              (format #f "ossia:~a:-1:s~a" kind source)))

#(define (ossia-header-value kind source value)
   (if (markup? value) (make-ossiaHeader-markup kind source value) value))

#(define (ossia-engraver context)
   (let ((staves '()))
     (make-engraver
       (acknowledgers
         ((grob-interface engraver grob source-engraver)
          (let* ((kind (grob::name grob))
                 (staff (ly:context-find (ly:translator-context source-engraver) 'Staff)))
            (when (and staff (not (memq staff staves)))
              (set! staves (append staves (list staff))))
            (when (memq kind '(NoteHead Rest MultiMeasureRest Clef KeySignature
                              TimeSignature DynamicText TextScript TextSpanner Script
                              Fingering RehearsalMark MetronomeMark Slur PhrasingSlur
                              Hairpin Tie TextMark SectionLabel BarLine))
              (let* ((original (ly:grob-property-data grob 'stencil))
                     (staff-index (if staff (list-index (lambda (s) (eq? s staff)) staves) -1))
                     (explicit (and staff (assq-ref (hashq-ref ossia-origins staff '()) kind)))
                     (origin (ossia-origin grob))
                     (where (and (ly:input-location? origin)
                                 (let ((location (ly:input-file-line-char-column origin)))
                                   (and (string-suffix? "score.ly" (car location)) location))))
                     (location (cond (explicit (string-append "s" explicit))
                                     (where (format #f "~a:~a" (cadr where) (caddr where)))
                                     (else "-1:-1")))
                     (url (format #f "ossia:~a:~a:~a" kind staff-index location)))
                (ly:grob-set-property! grob 'stencil
                  (lambda (grob)
                    (ossia-box (ly:unpure-call original grob) url grob)))))))))))

\layout { \context { \Score \consists #ossia-engraver } }
