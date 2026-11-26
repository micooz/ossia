use super::*;

#[test]
fn ledger_line_notes_keep_their_system() {
    let lines = (16..=20)
        .rev()
        .map(|y| SvgElement {
            kind: "line".into(),
            bbox: BBox {
                x0: 10.,
                y0: y as f64,
                x1: 50.,
                y1: y as f64,
            },
            anchor: None,
        })
        .collect::<Vec<_>>();
    let note = SvgObject {
        note: None,
        timing: None,
        ornament: None,
        kind: "NoteHead".into(),
        bbox: BBox {
            x0: 20.,
            y0: 21.5,
            x1: 21.5,
            y1: 22.5,
        },
        span: None,
        staff_index: Some(0),
        hit_only: false,
        graphic_id: None,
        owners: Vec::new(),
        related: Vec::new(),
    };
    let (staff_space, systems) = systems(&lines, &[note], &[StaffKind::Notation], false).unwrap();
    assert_eq!(staff_space, 1.);
    assert_eq!(systems.len(), 1);
    assert_eq!(systems[0].staves.len(), 1);
    assert_eq!((systems[0].top, systems[0].bot), (16., 20.));
}

#[test]
fn staff_ids_outside_the_system_group_fall_back_to_geometry() {
    let lines = (10..=14)
        .chain(20..=24)
        .chain(40..=44)
        .chain(50..=54)
        .map(|y| SvgElement {
            kind: "line".into(),
            bbox: BBox {
                x0: 10.,
                y0: y as f64,
                x1: 90.,
                y1: y as f64,
            },
            anchor: None,
        })
        .collect::<Vec<_>>();
    let note = |staff_index, y| SvgObject {
        note: None,
        timing: None,
        ornament: None,
        kind: "NoteHead".into(),
        bbox: BBox {
            x0: 20.,
            y0: y,
            x1: 21.5,
            y1: y + 1.,
        },
        span: None,
        staff_index: Some(staff_index),
        hit_only: false,
        graphic_id: None,
        owners: Vec::new(),
        related: Vec::new(),
    };
    let notes = [note(2, 21.5), note(3, 51.5)];
    let (_, systems) = systems(
        &lines,
        &notes,
        &[StaffKind::Notation, StaffKind::Notation],
        false,
    )
    .unwrap();
    assert_eq!(systems.len(), 2);
}
