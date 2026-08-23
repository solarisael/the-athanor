pub(crate) fn chunk_body(body: &str) -> Vec<(String, usize, usize, Option<String>)> {
    if body.is_empty() {
        return vec![];
    }
    let chars: Vec<char> = body.chars().collect();
    let mut headings: Vec<(usize, String)> = Vec::new();
    let mut offset = 0usize;
    for line in body.split_inclusive('\n') {
        let content = line.trim_end_matches('\n');
        if content
            .strip_prefix("## ")
            .is_some_and(|h| !h.trim().is_empty())
        {
            headings.push((offset, content.trim().to_string()));
        }
        offset += line.chars().count();
    }
    let mut sections = Vec::new();
    if headings.is_empty() {
        sections.push((0, chars.len(), "__preamble__".to_string()));
    } else {
        if headings[0].0 > 0 {
            sections.push((0, headings[0].0, "__preamble__".to_string()));
        }
        for (i, (start, heading)) in headings.iter().enumerate() {
            sections.push((
                *start,
                headings.get(i + 1).map(|(s, _)| *s).unwrap_or(chars.len()),
                heading.clone(),
            ));
        }
    }
    let byte_at = |char_index: usize| -> usize {
        body.char_indices()
            .nth(char_index)
            .map(|(i, _)| i)
            .unwrap_or(body.len())
    };
    let mut out = Vec::new();
    for (start, end, heading) in sections {
        let text: String = chars[start..end].iter().collect();
        if text.chars().count() <= 4000 {
            if !text.trim().is_empty() {
                out.push((text, byte_at(start), byte_at(end), Some(heading)));
            }
            continue;
        }
        let mut paragraphs = Vec::new();
        let mut paragraph_start = start;
        for i in start..end.saturating_sub(1) {
            if chars[i] == '\n' && chars[i + 1] == '\n' {
                paragraphs.push((paragraph_start, i));
                paragraph_start = i + 2;
            }
        }
        paragraphs.push((paragraph_start, end));
        let mut pieces = Vec::new();
        let mut buf_start = paragraphs[0].0;
        let mut buf_end = paragraphs[0].1;
        for &(paragraph_start, paragraph_end) in paragraphs.iter().skip(1) {
            if buf_end - buf_start + (paragraph_end - paragraph_start) + 2 > 2200 {
                pieces.push((buf_start, buf_end));
                buf_start = buf_end.saturating_sub(200);
                buf_end = paragraph_end;
            } else {
                buf_end = paragraph_end;
            }
        }
        pieces.push((buf_start, buf_end));
        for (piece_start, piece_end) in pieces {
            let piece: String = chars[piece_start..piece_end].iter().collect();
            if !piece.trim().is_empty() {
                out.push((
                    piece,
                    byte_at(piece_start),
                    byte_at(piece_end),
                    Some(heading.clone()),
                ));
            }
        }
    }
    out
}
