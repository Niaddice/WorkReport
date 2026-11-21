use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiffLineKind {
    Context,
    Add,
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiffLine {
    pub(crate) kind: DiffLineKind,
    pub(crate) old_line: Option<usize>,
    pub(crate) new_line: Option<usize>,
    pub(crate) text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SplitSideKind {
    Context,
    Add,
    Remove,
    Empty,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SplitDiffSide {
    pub(crate) line: Option<usize>,
    pub(crate) text: String,
    pub(crate) kind: SplitSideKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SplitDiffRow {
    pub(crate) left: SplitDiffSide,
    pub(crate) right: SplitDiffSide,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiffResult {
    pub(crate) lines: Vec<DiffLine>,
    pub(crate) split_rows: Vec<SplitDiffRow>,
    pub(crate) additions: usize,
    pub(crate) deletions: usize,
    pub(crate) identical: bool,
}

fn split_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .map(str::to_owned)
        .collect()
}

#[derive(Clone, Copy)]
enum DiffOp {
    Equal,
    Add,
    Remove,
}

fn myers_diff<'a>(old_lines: &'a [String], new_lines: &'a [String]) -> Vec<(DiffOp, &'a str)> {
    let n = old_lines.len();
    let m = new_lines.len();
    let max = n + m;
    if n == 0 && m == 0 {
        return Vec::new();
    }
    if n == 0 {
        return new_lines
            .iter()
            .map(|text| (DiffOp::Add, text.as_str()))
            .collect();
    }
    if m == 0 {
        return old_lines
            .iter()
            .map(|text| (DiffOp::Remove, text.as_str()))
            .collect();
    }

    let mut v: HashMap<isize, usize> = HashMap::new();
    v.insert(1, 0);
    let mut trace: Vec<HashMap<isize, usize>> = Vec::with_capacity(max + 1);
    let mut found_d = 0;

    'outer: for d in 0..=max {
        trace.push(v.clone());
        let d_i = d as isize;
        let mut k = -d_i;
        while k <= d_i {
            let mut x = if k == -d_i
                || (k != d_i
                    && v.get(&(k - 1)).copied().unwrap_or(0)
                        < v.get(&(k + 1)).copied().unwrap_or(0))
            {
                v.get(&(k + 1)).copied().unwrap_or(0)
            } else {
                v.get(&(k - 1)).copied().unwrap_or(0) + 1
            };
            let mut y = (x as isize - k) as usize;
            while x < n && y < m && old_lines[x] == new_lines[y] {
                x += 1;
                y += 1;
            }
            v.insert(k, x);
            if x >= n && y >= m {
                found_d = d;
                break 'outer;
            }
            k += 2;
        }
    }

    let mut script = Vec::new();
    let mut x = n;
    let mut y = m;
    for d in (1..=found_d).rev() {
        let v_prev = &trace[d];
        let d_i = d as isize;
        let k = x as isize - y as isize;
        let prev_k = if k == -d_i
            || (k != d_i
                && v_prev.get(&(k - 1)).copied().unwrap_or(0)
                    < v_prev.get(&(k + 1)).copied().unwrap_or(0))
        {
            k + 1
        } else {
            k - 1
        };
        let prev_x = v_prev.get(&prev_k).copied().unwrap_or(0);
        let prev_y = (prev_x as isize - prev_k) as usize;

        while x > prev_x && y > prev_y {
            script.push((DiffOp::Equal, old_lines[x - 1].as_str()));
            x -= 1;
            y -= 1;
        }
        if x == prev_x {
            script.push((DiffOp::Add, new_lines[y - 1].as_str()));
            y -= 1;
        } else {
            script.push((DiffOp::Remove, old_lines[x - 1].as_str()));
            x -= 1;
        }
    }
    while x > 0 && y > 0 {
        script.push((DiffOp::Equal, old_lines[x - 1].as_str()));
        x -= 1;
        y -= 1;
    }
    script.reverse();
    script
}

fn build_split_rows(lines: &[DiffLine]) -> Vec<SplitDiffRow> {
    let mut rows = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let current = &lines[i];
        if current.kind == DiffLineKind::Context {
            rows.push(SplitDiffRow {
                left: SplitDiffSide {
                    line: current.old_line,
                    text: current.text.clone(),
                    kind: SplitSideKind::Context,
                },
                right: SplitDiffSide {
                    line: current.new_line,
                    text: current.text.clone(),
                    kind: SplitSideKind::Context,
                },
            });
            i += 1;
            continue;
        }

        let mut removes = Vec::new();
        let mut adds = Vec::new();
        while i < lines.len() && lines[i].kind != DiffLineKind::Context {
            if lines[i].kind == DiffLineKind::Remove {
                removes.push(&lines[i]);
            } else {
                adds.push(&lines[i]);
            }
            i += 1;
        }
        let max_count = removes.len().max(adds.len());
        for j in 0..max_count {
            let left = removes.get(j).map_or_else(
                || SplitDiffSide {
                    line: None,
                    text: String::new(),
                    kind: SplitSideKind::Empty,
                },
                |line| SplitDiffSide {
                    line: line.old_line,
                    text: line.text.clone(),
                    kind: SplitSideKind::Remove,
                },
            );
            let right = adds.get(j).map_or_else(
                || SplitDiffSide {
                    line: None,
                    text: String::new(),
                    kind: SplitSideKind::Empty,
                },
                |line| SplitDiffSide {
                    line: line.new_line,
                    text: line.text.clone(),
                    kind: SplitSideKind::Add,
                },
            );
            rows.push(SplitDiffRow { left, right });
        }
    }
    rows
}

pub(crate) fn compute_line_diff(old_text: &str, new_text: &str) -> DiffResult {
    if old_text == new_text {
        let lines = split_lines(old_text);
        let diff_lines: Vec<DiffLine> = lines
            .into_iter()
            .enumerate()
            .map(|(idx, text)| DiffLine {
                kind: DiffLineKind::Context,
                old_line: Some(idx + 1),
                new_line: Some(idx + 1),
                text,
            })
            .collect();
        let split_rows = diff_lines
            .iter()
            .map(|item| SplitDiffRow {
                left: SplitDiffSide {
                    line: item.old_line,
                    text: item.text.clone(),
                    kind: SplitSideKind::Context,
                },
                right: SplitDiffSide {
                    line: item.new_line,
                    text: item.text.clone(),
                    kind: SplitSideKind::Context,
                },
            })
            .collect();
        return DiffResult {
            lines: diff_lines,
            split_rows,
            additions: 0,
            deletions: 0,
            identical: true,
        };
    }

    let old_lines = split_lines(old_text);
    let new_lines = split_lines(new_text);

    let mut start = 0;
    while start < old_lines.len()
        && start < new_lines.len()
        && old_lines[start] == new_lines[start]
    {
        start += 1;
    }

    let mut old_end = old_lines.len();
    let mut new_end = new_lines.len();
    while old_end > start && new_end > start && old_lines[old_end - 1] == new_lines[new_end - 1] {
        old_end -= 1;
        new_end -= 1;
    }

    let prefix = &old_lines[..start];
    let suffix = &old_lines[old_end..];
    let middle_old = &old_lines[start..old_end];
    let middle_new = &new_lines[start..new_end];
    let middle_script = myers_diff(middle_old, middle_new);

    let mut old_line_num = 1;
    let mut new_line_num = 1;
    let mut additions = 0;
    let mut deletions = 0;
    let mut diff_lines = Vec::new();

    for text in prefix {
        diff_lines.push(DiffLine {
            kind: DiffLineKind::Context,
            old_line: Some(old_line_num),
            new_line: Some(new_line_num),
            text: text.clone(),
        });
        old_line_num += 1;
        new_line_num += 1;
    }

    for (op, text) in middle_script {
        match op {
            DiffOp::Equal => {
                diff_lines.push(DiffLine {
                    kind: DiffLineKind::Context,
                    old_line: Some(old_line_num),
                    new_line: Some(new_line_num),
                    text: text.to_owned(),
                });
                old_line_num += 1;
                new_line_num += 1;
            }
            DiffOp::Remove => {
                diff_lines.push(DiffLine {
                    kind: DiffLineKind::Remove,
                    old_line: Some(old_line_num),
                    new_line: None,
                    text: text.to_owned(),
                });
                old_line_num += 1;
                deletions += 1;
            }
            DiffOp::Add => {
                diff_lines.push(DiffLine {
                    kind: DiffLineKind::Add,
                    old_line: None,
                    new_line: Some(new_line_num),
                    text: text.to_owned(),
                });
                new_line_num += 1;
                additions += 1;
            }
        }
    }

    for text in suffix {
        diff_lines.push(DiffLine {
            kind: DiffLineKind::Context,
            old_line: Some(old_line_num),
            new_line: Some(new_line_num),
            text: text.clone(),
        });
        old_line_num += 1;
        new_line_num += 1;
    }

    let split_rows = build_split_rows(&diff_lines);
    DiffResult {
        lines: diff_lines,
        split_rows,
        additions,
        deletions,
        identical: additions == 0 && deletions == 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_and_crlf_only_changes_are_detected_correctly() {
        let same = compute_line_diff("第一行\r\n第二行 📝", "第一行\n第二行 📝");
        assert!(same.identical);
        assert_eq!(same.additions, 0);
        assert_eq!(same.deletions, 0);
        assert_eq!(same.lines.len(), 2);

        let diff = compute_line_diff("标题\n旧内容\n结尾", "标题\n新内容 📝\n补充一行\n结尾");
        assert!(!diff.identical);
        assert_eq!(diff.deletions, 1);
        assert_eq!(diff.additions, 2);
        assert_eq!(diff.split_rows.len(), 4);
        assert_eq!(diff.split_rows[1].left.kind, SplitSideKind::Remove);
        assert_eq!(diff.split_rows[1].right.kind, SplitSideKind::Add);
        assert_eq!(diff.split_rows[2].left.kind, SplitSideKind::Empty);
        assert_eq!(diff.split_rows[2].right.kind, SplitSideKind::Add);
    }
}
