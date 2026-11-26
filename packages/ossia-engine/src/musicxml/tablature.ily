#(define (ossia-xml-tab-assignments ctx event notes assignments)
   (let ((strings (length (ly:context-property ctx 'stringTunings '()))))
     ;; LilyPond returns one assignment per input note, in the same order.
     (if (and (list? assignments) (= (length notes) (length assignments))
              (every (lambda (entry)
                       (and (pair? entry) (pair? (cdr entry))
                            (integer? (car entry)) (<= 1 (car entry) strings)
                            (integer? (cadr entry)) (>= (cadr entry) 0)))
                     assignments))
         (for-each
          (lambda (note entry)
            (ossia-xml-emit 'tab-note ctx note
                            (ossia-xml-pitch (ly:event-property note 'pitch))
                            (car entry) (cadr entry)))
          notes assignments)
         (ossia-xml-unsupported ctx event "a tablature note without a playable integer fret"))))

#(define (ossia-xml-tab-voice ctx)
   (let ((original #f) (local? #f))
     (make-engraver
      (listeners
       ((note-event e event)
        (unless original
          (let ((callback (ly:context-property ctx 'noteToFretFunction)))
            (if (not (procedure? callback))
                (ossia-xml-unsupported ctx event "tablature without a fret assignment function")
                (begin
                  (set! original callback)
                  (set! local? (eq? ctx (ly:context-property-where-defined ctx 'noteToFretFunction)))
                  (ly:context-set-property! ctx 'noteToFretFunction
                    (lambda (context notes specified)
                      (let ((result (original context notes specified)))
                        (ossia-xml-tab-assignments ctx event notes result)
                        result)))))))))
      ((stop-translation-timestep e)
       (when original
         (if local? (ly:context-set-property! ctx 'noteToFretFunction original)
             (ly:context-unset-property ctx 'noteToFretFunction))
         (set! original #f))))))

#(define (ossia-xml-tab-staff ctx)
   (let ((previous #f))
     (make-engraver
      ((stop-translation-timestep e)
       (let ((tuning (ly:context-property ctx 'stringTunings))
             (additional (ly:context-property ctx 'additionalBassStrings '())))
         (if (or (not (pair? tuning)) (not (every ly:pitch? tuning))
                 (not (ly:context-property ctx 'stringOneTopmost))
                 (not (eq? (ly:context-property ctx 'tabStaffLineLayoutFunction)
                           tablature-position-on-lines))
                 (pair? additional))
             (ossia-xml-unsupported ctx #f "a nonstandard tablature string layout")
             (unless (equal? tuning previous)
               (set! previous tuning)
               (ossia-xml-emit 'tab-tuning ctx #f (map ossia-xml-pitch tuning)))))))))

#(define (ossia-xml-fretboards ctx)
   (make-engraver
    (acknowledgers
     ((fret-diagram-interface e grob source)
      (let ((details (ly:grob-property grob 'fret-diagram-details)))
        (ossia-xml-emit 'fretboard ctx (ly:grob-property grob 'cause)
                        (assoc-get 'string-count details 6)
                        (assoc-get 'fret-count details 4)
                        (ly:grob-property grob 'dot-placement-list)))))))
