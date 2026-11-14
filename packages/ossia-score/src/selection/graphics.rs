use super::*;
use std::collections::{BTreeMap, BTreeSet};

fn key(target: &SelectionTarget) -> String {
    match target {
        SelectionTarget::Element { element_id } => element_id.clone(),
        SelectionTarget::Measure { .. } => unreachable!("only SVG elements have graphics"),
    }
}

/// Resolve engraving-object ownership before joining repeated source occurrences.
pub(super) fn associate(index: &ScoreIndex, layout: &ScoreLayout, pages: &mut [GeometryPage]) {
    let objects: Vec<_> = layout
        .pages
        .iter()
        .flat_map(|p| {
            p.objects
                .iter()
                .filter(|o| !o.hit_only)
                .map(move |o| (p, o))
        })
        .collect();
    let by_id: BTreeMap<_, _> = objects
        .iter()
        .filter_map(|(page, object)| {
            Some((
                object.graphic_id.as_deref()?,
                object_region(index, page, object)?,
            ))
        })
        .collect();
    let notes: BTreeMap<_, _> = by_id
        .iter()
        .filter(|(_, r)| r.element_kind.as_deref() == Some("NoteHead"))
        .map(|(id, r)| (*id, key(&r.target)))
        .collect();
    let mut edges = BTreeMap::<String, BTreeSet<String>>::new();
    let mut attachments = BTreeMap::<String, BTreeSet<String>>::new();
    let mut unmapped = BTreeMap::<String, BTreeSet<String>>::new();
    for (_, object) in &objects {
        let owners: Vec<_> = object
            .owners
            .iter()
            .filter_map(|id| notes.get(id.as_str()))
            .collect();
        if matches!(
            object.kind.as_str(),
            "Tie" | "Script" | "Fingering" | "TextScript"
        ) {
            let mapped = object
                .graphic_id
                .as_deref()
                .and_then(|id| by_id.get(id))
                .is_some_and(|r| !r.spans.is_empty());
            if !mapped || (object.kind == "Tie" && owners.len() != 2) {
                for owner in &owners {
                    unmapped
                        .entry((*owner).clone())
                        .or_default()
                        .insert(object.kind.clone());
                }
            }
        }
        if object.kind == "Tie" {
            for a in &owners {
                edges
                    .entry((*a).clone())
                    .or_default()
                    .extend(owners.iter().map(|b| (*b).clone()));
            }
        }
        if object.kind == "NoteHead" {
            if let Some(own) = object.graphic_id.as_deref().and_then(|id| notes.get(id)) {
                attachments.entry(own.clone()).or_default().extend(
                    object
                        .related
                        .iter()
                        .filter_map(|id| notes.get(id.as_str()).cloned()),
                );
            }
        }
        if matches!(
            object.kind.as_str(),
            "Tie" | "Script" | "Fingering" | "TextScript"
        ) {
            if let Some(region) = object.graphic_id.as_deref().and_then(|id| by_id.get(id)) {
                if !region.spans.is_empty() {
                    for owner in owners {
                        attachments
                            .entry(owner.clone())
                            .or_default()
                            .insert(key(&region.target));
                    }
                }
            }
        }
    }
    let targets: BTreeMap<_, _> = by_id
        .values()
        .map(|r| (key(&r.target), r.target.clone()))
        .collect();
    for page in pages.iter_mut() {
        for element in &mut page.elements {
            let own = key(&element.region.target);
            let is_note = element.region.element_kind.as_deref() == Some("NoteHead");
            if is_note {
                let mut chain = BTreeSet::from([own.clone()]);
                let mut pending = vec![own.clone()];
                while let Some(current) = pending.pop() {
                    for next in edges.get(&current).into_iter().flatten() {
                        if chain.insert(next.clone()) {
                            pending.push(next.clone());
                        }
                    }
                }
                let mut linked = chain.clone();
                for note in chain {
                    linked.extend(attachments.get(&note).into_iter().flatten().cloned());
                }
                element.linked_targets = linked
                    .iter()
                    .filter_map(|id| targets.get(id).cloned())
                    .collect();
            }
            // Ownership of a shared beam affects its ink, never its other notes' targets.
            for (_, object) in objects.iter().filter(|(p, _)| p.page == page.page) {
                if object
                    .owners
                    .iter()
                    .any(|id| notes.get(id.as_str()) == Some(&own))
                {
                    if let Some(id) = &object.graphic_id {
                        if !element.graphic_ids.contains(id) {
                            element.graphic_ids.push(id.clone());
                        }
                    }
                    if is_note
                        && matches!(
                            object.kind.as_str(),
                            "Stem"
                                | "Flag"
                                | "Dots"
                                | "Accidental"
                                | "AccidentalCautionary"
                                | "StemTremolo"
                        )
                    {
                        if element.hit_bounds.is_empty() {
                            element.hit_bounds.push(element.bounds.clone());
                        }
                        element.hit_bounds.push(bounds(object.bbox));
                    }
                }
            }
            if is_note && element.region.reason.is_none() {
                if let Some(kinds) = unmapped.get(&own) {
                    element.region.reason = Some(format!("Some associated objects have no reliable source mapping or complete endpoints: {}", kinds.iter().cloned().collect::<Vec<_>>().join(", ")));
                }
            }
            if is_note && element.graphic_ids.is_empty() && element.region.reason.is_none() {
                element.region.reason =
                    Some("This render has no reliable graphic ownership metadata".into());
            }
        }
    }
    // A main note can own grace notes that have attachments of their own.
    let expanded: BTreeMap<_, _> = pages
        .iter()
        .flat_map(|p| &p.elements)
        .filter(|e| e.region.element_kind.as_deref() == Some("NoteHead"))
        .map(|e| {
            (
                key(&e.region.target),
                expand_selection(pages, std::slice::from_ref(&e.region.target)),
            )
        })
        .collect();
    for page in pages {
        for element in &mut page.elements {
            if let Some(linked) = expanded.get(&key(&element.region.target)) {
                element.linked_targets = linked.clone();
            }
        }
    }
}

/// Expand browser or Agent targets using only current, authoritative engraving links.
pub fn expand_selection(
    pages: &[GeometryPage],
    targets: &[SelectionTarget],
) -> Vec<SelectionTarget> {
    let mut result = Vec::new();
    let mut pending = targets.to_vec();
    let mut cursor = 0;
    while let Some(target) = pending.get(cursor).cloned() {
        cursor += 1;
        if result.contains(&target) {
            continue;
        }
        if let Some(element) = pages
            .iter()
            .flat_map(|p| &p.elements)
            .find(|e| e.region.target == target)
        {
            pending.extend(
                element
                    .linked_targets
                    .iter()
                    .filter(|t| **t != target)
                    .cloned(),
            );
        }
        result.push(target);
    }
    result
}
