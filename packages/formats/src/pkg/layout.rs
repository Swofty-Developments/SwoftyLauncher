#[derive(Debug, Clone)]
pub struct Layout {
    pub(super) chunks: Vec<(u32, u32)>,
    pub(super) gaps: Vec<(u32, Vec<u8>)>,
    pub(super) end: u32,
    pub(super) tail: Vec<u8>,
}

impl Layout {
    pub(super) fn of(data: &[u8], chunks: Vec<(u32, u32)>, end: u32, tail: &[u8]) -> Option<Self> {
        let canonical = chunks.first().is_some_and(|&(at, _)| at == 16)
            && chunks.windows(2).all(|w| w[0].0 + w[0].1 == w[1].0)
            && chunks
                .last()
                .is_some_and(|&(at, n)| (at + n).next_multiple_of(4) == end)
            && data.len().is_multiple_of(4)
            && tail.len() < 4
            && tail.iter().all(|&b| b == 0);
        if canonical {
            return None;
        }
        let mut spans = chunks.clone();
        spans.sort_unstable();
        let (mut pos, mut gaps) = (16, Vec::new());
        for &(at, n) in spans.iter().chain([&(end, 0)]) {
            if at < pos {
                return None;
            }
            if at > pos {
                gaps.push((pos, data[pos as usize..at as usize].to_vec()));
            }
            pos = at + n;
        }
        Some(Self {
            chunks,
            gaps,
            end,
            tail: tail.to_vec(),
        })
    }
}
