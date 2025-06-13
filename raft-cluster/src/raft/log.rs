use crate::pb::LogEntry;

pub struct RaftLog {
    pub entities: Vec<LogEntry>, // 日志条目
    pub commit_index: u64,       // 已提交的日志索引
    pub last_applied: u64,       // 已应用的日志索引
}

impl RaftLog {
    pub fn new() -> Self {
        Self {
            entities: Vec::new(),
            commit_index: 0,
            last_applied: 0,
        }
    }

    pub fn last_log_index(&self) -> u64 {
        self.entities.last().map(|e| e.index).unwrap_or(0)
    }

    pub fn last_log_term(&self) -> u64 {
        self.entities.last().map(|e| e.term).unwrap_or(0)
    }

    pub fn append_entries(&mut self, entries: Vec<LogEntry>) {
        self.entities.extend(entries);
    }

    pub fn append_entry(&mut self, entry: LogEntry) {
        self.entities.push(entry);
    }

    /// 从指定索引开始获取日志条目
    pub fn get_entries_from(&self, start_index: u64) -> Vec<LogEntry> {
        self.entities
            .iter()
            .filter(|entry| entry.index >= start_index)
            .cloned()
            .collect()
    }

    /// 获取指定索引的日志条目的任期
    pub fn get_term_at(&self, index: u64) -> Option<u64> {
        if index == 0 {
            return Some(0); // 索引0的任期默认为0
        }
        
        self.entities
            .iter()
            .find(|entry| entry.index == index)
            .map(|entry| entry.term)
    }

    /// 获取指定索引的日志条目
    pub fn get_entry_at(&self, index: u64) -> Option<&LogEntry> {
        self.entities
            .iter()
            .find(|entry| entry.index == index)
    }

    /// 检查是否包含指定索引和任期的日志条目
    pub fn contains_entry(&self, index: u64, term: u64) -> bool {
        self.entities
            .iter()
            .any(|entry| entry.index == index && entry.term == term)
    }

    /// 删除从指定索引开始的所有日志条目（用于处理冲突）
    pub fn truncate_from(&mut self, start_index: u64) {
        self.entities.retain(|entry| entry.index < start_index);
    }
}
