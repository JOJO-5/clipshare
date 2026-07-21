use crate::wechat::WeChatMessage;
use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiMessageNode {
    pub runtime_id: String,
    pub sender: String,
    pub content: String,
    pub incoming: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct UiScanSummary {
    windows: usize,
    sessions: usize,
    unread_sessions: usize,
    opened_sessions: usize,
    lists: usize,
    items: usize,
    parsed_nodes: usize,
    incoming_nodes: usize,
    text_nodes: usize,
    messages: usize,
}

impl UiScanSummary {
    fn format_log(&self) -> String {
        format!(
            "wechat-ui windows={} sessions={} unread_sessions={} opened_sessions={} lists={} items={} parsed={} incoming={} text_nodes={} messages={}",
            self.windows,
            self.sessions,
            self.unread_sessions,
            self.opened_sessions,
            self.lists,
            self.items,
            self.parsed_nodes,
            self.incoming_nodes,
            self.text_nodes,
            self.messages,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WeChatSession {
    name: String,
    unread_count: usize,
}

fn parse_unread_session_label(label: &str) -> Option<WeChatSession> {
    let marker = "条新消息";
    let marker_start = label.find(marker)?;
    let digits = label[..marker_start]
        .chars()
        .rev()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    let unread_count = digits.parse::<usize>().ok()?;
    if unread_count == 0 {
        return None;
    }

    let name_end = marker_start.saturating_sub(digits.len());
    let name = label[..name_end]
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .last()
        .unwrap_or_default();
    if name.is_empty() {
        return None;
    }

    Some(WeChatSession {
        name: name.to_string(),
        unread_count,
    })
}

fn parse_unread_session_item(label: Option<&str>, text_names: &[String]) -> Option<WeChatSession> {
    let candidates = label
        .into_iter()
        .chain(text_names.iter().map(String::as_str))
        .collect::<Vec<_>>();
    if let Some(session) = candidates
        .iter()
        .find_map(|candidate| parse_unread_session_label(candidate))
    {
        return Some(session);
    }

    let unread_count = candidates.iter().find_map(|candidate| {
        let marker_start = candidate.find("条新消息")?;
        let digits = candidate[..marker_start]
            .chars()
            .rev()
            .take_while(|character| character.is_ascii_digit())
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        digits.parse::<usize>().ok()
    })?;
    if unread_count == 0 {
        return None;
    }
    let name = candidates
        .iter()
        .map(|candidate| candidate.trim())
        .find(|candidate| !candidate.is_empty() && !candidate.contains("条新消息"))?;
    Some(WeChatSession {
        name: name.to_string(),
        unread_count,
    })
}

fn select_latest_messages<T>(items: &[T], count: usize) -> &[T] {
    let start = items.len().saturating_sub(count);
    &items[start..]
}

fn should_scan_wechat_fallback(summary: &UiScanSummary) -> bool {
    summary.parsed_nodes == 0 && summary.messages == 0 && summary.unread_sessions == 0
}

pub fn messages_from_nodes(nodes: &[UiMessageNode]) -> Vec<WeChatMessage> {
    nodes
        .iter()
        .filter(|node| {
            node.incoming && !node.sender.trim().is_empty() && !node.content.trim().is_empty()
        })
        .map(|node| WeChatMessage {
            id: format!("wechat-ui-{}", node.runtime_id),
            sender: node.sender.trim().to_string(),
            preview: String::new(),
            content: node.content.trim().to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            unread_count: 1,
        })
        .collect()
}

#[derive(Debug, Default)]
pub struct MessageTracker {
    initialized: bool,
    delivered_ids: HashSet<String>,
}

impl MessageTracker {
    pub fn ingest(&mut self, messages: Vec<WeChatMessage>) -> Vec<WeChatMessage> {
        if !self.initialized {
            self.delivered_ids
                .extend(messages.into_iter().map(|message| message.id));
            self.initialized = true;
            return Vec::new();
        }

        let mut batch_ids = HashSet::new();
        messages
            .into_iter()
            .filter(|message| {
                !self.delivered_ids.contains(&message.id) && batch_ids.insert(message.id.clone())
            })
            .collect()
    }

    pub fn mark_delivered(&mut self, id: &str) {
        self.delivered_ids.insert(id.to_string());
    }
}

pub fn extract_latest_message(texts: &[String]) -> Option<WeChatMessage> {
    let non_empty: Vec<&str> = texts
        .iter()
        .map(String::as_str)
        .filter(|text| !text.trim().is_empty())
        .collect();
    if non_empty.len() < 3 {
        return None;
    }

    let sender = non_empty.get(1)?.trim();
    let content = non_empty.last()?.trim();
    if sender.is_empty() || content.is_empty() || content == sender {
        return None;
    }

    Some(WeChatMessage {
        id: format!("wechat-{}-{}", sender, content),
        sender: sender.to_string(),
        preview: String::new(),
        content: content.to_string(),
        timestamp: chrono::Utc::now().timestamp(),
        unread_count: 1,
    })
}

fn select_message_content(
    item_name: Option<&str>,
    text_names: &[String],
    sender: &str,
) -> Option<String> {
    text_names
        .iter()
        .rev()
        .map(String::as_str)
        .map(str::trim)
        .find(|text| !text.is_empty() && *text != sender)
        .or_else(|| {
            item_name
                .map(str::trim)
                .filter(|text| !text.is_empty() && *text != sender)
        })
        .map(str::to_string)
}

pub struct WeChatMonitor {
    stop: Arc<AtomicBool>,
}

impl WeChatMonitor {
    pub fn start<F, D>(on_message: F, on_diagnostic: D) -> Result<Self, String>
    where
        F: Fn(WeChatMessage) -> bool + Send + 'static,
        D: Fn(String) + Send + 'static,
    {
        #[cfg(windows)]
        {
            let stop = Arc::new(AtomicBool::new(false));
            let thread_stop = Arc::clone(&stop);
            thread::spawn(move || {
                let automation = match uiautomation::UIAutomation::new() {
                    Ok(automation) => automation,
                    Err(error) => {
                        on_diagnostic(format!("wechat-monitor UIA initialization failed: {error}"));
                        return;
                    }
                };
                on_diagnostic("wechat-monitor UIA initialized".to_string());
                let mut tracker = MessageTracker::default();
                let mut last_summary = None;
                let mut last_summary_log = Instant::now() - Duration::from_secs(5);
                let mut last_pending_log = Instant::now() - Duration::from_secs(5);

                while !thread_stop.load(Ordering::Relaxed) {
                    let (messages, summary) = scan_wechat(&automation);
                    if last_summary.as_ref() != Some(&summary)
                        || last_summary_log.elapsed() >= Duration::from_secs(5)
                    {
                        on_diagnostic(summary.format_log());
                        last_summary = Some(summary);
                        last_summary_log = Instant::now();
                    }

                    let pending = tracker.ingest(messages);
                    if !pending.is_empty() && last_pending_log.elapsed() >= Duration::from_secs(5) {
                        on_diagnostic(format!(
                            "wechat-monitor incoming messages pending_delivery={}",
                            pending.len()
                        ));
                        last_pending_log = Instant::now();
                    }
                    for message in pending {
                        if on_message(message.clone()) {
                            tracker.mark_delivered(&message.id);
                        }
                    }
                    thread::sleep(Duration::from_millis(500));
                }
            });
            return Ok(Self { stop });
        }

        #[cfg(not(windows))]
        {
            let _ = on_message;
            let _ = on_diagnostic;
            Err("微信监听仅支持 Windows".to_string())
        }
    }
}

impl Drop for WeChatMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(windows)]
fn scan_wechat(automation: &uiautomation::UIAutomation) -> (Vec<WeChatMessage>, UiScanSummary) {
    let main_windows = find_wechat_main_windows(automation);
    let mut summary = UiScanSummary {
        windows: main_windows.len(),
        ..UiScanSummary::default()
    };
    let mut messages = Vec::new();
    for window in main_windows {
        let (window_messages, window_summary) = scan_wechat_window(automation, &window);
        merge_scan_summary(&mut summary, &window_summary);
        messages.extend(window_messages);
    }
    if should_scan_wechat_fallback(&summary) {
        for window in find_wechat_windows(automation) {
            let (window_messages, window_summary) = scan_wechat_window(automation, &window);
            summary.windows += 1;
            merge_scan_summary(&mut summary, &window_summary);
            messages.extend(window_messages);
        }
    }
    summary.messages = messages.len();
    (messages, summary)
}

#[cfg(windows)]
fn merge_scan_summary(target: &mut UiScanSummary, source: &UiScanSummary) {
    target.sessions += source.sessions;
    target.unread_sessions += source.unread_sessions;
    target.opened_sessions += source.opened_sessions;
    target.lists += source.lists;
    target.items += source.items;
    target.parsed_nodes += source.parsed_nodes;
    target.incoming_nodes += source.incoming_nodes;
    target.text_nodes += source.text_nodes;
}

#[cfg(windows)]
fn find_wechat_main_windows(
    automation: &uiautomation::UIAutomation,
) -> Vec<uiautomation::UIElement> {
    automation
        .create_matcher()
        .classname("WeChatMainWndForPC")
        .timeout(300)
        .find_all()
        .unwrap_or_default()
}

#[cfg(windows)]
fn find_wechat_windows(automation: &uiautomation::UIAutomation) -> Vec<uiautomation::UIElement> {
    let direct_chat_windows = automation
        .create_matcher()
        .classname("ChatWnd")
        .depth(1)
        .timeout(300)
        .find_all()
        .unwrap_or_default();
    if !direct_chat_windows.is_empty() {
        return direct_chat_windows;
    }

    let mut windows = Vec::new();
    if let (Ok(root), Ok(walker)) = (
        automation.get_root_element(),
        automation.get_control_view_walker(),
    ) {
        if let Some(children) = walker.get_children(&root) {
            let top_level_chat_windows = children
                .iter()
                .filter(|window| window.get_classname().unwrap_or_default() == "ChatWnd")
                .cloned()
                .collect::<Vec<_>>();
            // wxauto locates ChatWnd at the desktop's first child level. It
            // is a real top-level window in several WeChat versions, so do
            // not require it to be nested below WeChatMainWndForPC.
            if !top_level_chat_windows.is_empty() {
                return top_level_chat_windows;
            }
            windows.extend(children.into_iter().filter(|window| {
                let class_name = window.get_classname().unwrap_or_default();
                let name = window.get_name().unwrap_or_default();
                class_name == "WeChatMainWndForPC"
                    || class_name.to_ascii_lowercase().contains("wechat")
                    || name.contains("微信")
                    || name.to_ascii_lowercase().contains("wechat")
            }));
        }
    }

    if !windows.is_empty() {
        let mut chat_windows = Vec::new();
        for window in windows {
            let class_name = window.get_classname().unwrap_or_default();
            if class_name == "WeChatMainWndForPC" {
                let children = automation
                    .create_matcher()
                    .from_ref(&window)
                    .classname("ChatWnd")
                    .depth(10)
                    .timeout(0)
                    .find_all()
                    .unwrap_or_default();
                if children.is_empty() {
                    chat_windows.push(window);
                } else {
                    chat_windows.extend(children);
                }
            } else {
                chat_windows.push(window);
            }
        }
        return chat_windows;
    }

    if windows.is_empty() {
        if let Ok(window) = automation
            .create_matcher()
            .classname("WeChatMainWndForPC")
            .timeout(300)
            .find_first()
        {
            windows.push(window);
        }
    }
    windows
}

#[cfg(windows)]
fn scan_wechat_window(
    automation: &uiautomation::UIAutomation,
    window: &uiautomation::UIElement,
) -> (Vec<WeChatMessage>, UiScanSummary) {
    let walker = match automation.get_control_view_walker() {
        Ok(walker) => walker,
        Err(_) => return (Vec::new(), UiScanSummary::default()),
    };
    let lists = automation
        .create_matcher()
        .from_ref(window)
        .control_type(uiautomation::controls::ControlType::List)
        .depth(12)
        .timeout(100)
        .find_all()
        .unwrap_or_default();

    if !lists.is_empty() {
        let items = lists
            .iter()
            .flat_map(|list| walker.get_children(list).unwrap_or_default())
            .collect::<Vec<_>>();
        let mut session_names = HashSet::new();
        let unread_targets = items
            .iter()
            .filter_map(|item| {
                let session = parse_unread_session_item_from_element(automation, &walker, item)?;
                if session_names.insert(session.name.clone()) {
                    Some((item, session))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let unread_sessions = unread_targets.len();
        let nodes = parse_message_nodes(automation, &items);
        let mut messages = messages_from_nodes(&nodes);
        let mut opened_sessions = 0;

        for (item, session) in unread_targets {
            if item.click().is_err() {
                continue;
            }
            opened_sessions += 1;
            thread::sleep(Duration::from_millis(120));
            let current_lists = automation
                .create_matcher()
                .from_ref(window)
                .control_type(uiautomation::controls::ControlType::List)
                .depth(12)
                .timeout(100)
                .find_all()
                .unwrap_or_default();
            let current_items = current_lists
                .iter()
                .flat_map(|list| walker.get_children(list).unwrap_or_default())
                .collect::<Vec<_>>();
            let current_nodes = parse_message_nodes(automation, &current_items);
            let latest_nodes = select_latest_messages(&current_nodes, session.unread_count);
            messages.extend(messages_from_nodes(latest_nodes));
        }

        return (
            messages.clone(),
            UiScanSummary {
                sessions: unread_sessions,
                unread_sessions,
                opened_sessions,
                lists: lists.len(),
                items: items.len(),
                parsed_nodes: nodes.len(),
                incoming_nodes: nodes.iter().filter(|node| node.incoming).count(),
                messages: messages.len(),
                ..UiScanSummary::default()
            },
        );
    }

    let mut texts = Vec::new();
    collect_ui_text(&walker, window, &mut texts);
    let messages = extract_latest_message(&texts)
        .into_iter()
        .collect::<Vec<_>>();
    (
        messages.clone(),
        UiScanSummary {
            text_nodes: texts.len(),
            messages: messages.len(),
            ..UiScanSummary::default()
        },
    )
}

#[cfg(windows)]
fn parse_unread_session_item_from_element(
    automation: &uiautomation::UIAutomation,
    walker: &uiautomation::UITreeWalker,
    item: &uiautomation::UIElement,
) -> Option<WeChatSession> {
    if item.get_control_type().ok()? != uiautomation::controls::ControlType::ListItem {
        return None;
    }
    let has_message_sender = automation
        .create_matcher()
        .from_ref(item)
        .control_type(uiautomation::controls::ControlType::Button)
        .depth(8)
        .timeout(0)
        .find_first()
        .is_ok();
    if has_message_sender {
        return None;
    }
    let mut text_names = Vec::new();
    collect_ui_text(walker, item, &mut text_names);
    parse_unread_session_item(item.get_name().ok().as_deref(), &text_names)
}

#[cfg(windows)]
fn parse_message_nodes(
    automation: &uiautomation::UIAutomation,
    items: &[uiautomation::UIElement],
) -> Vec<UiMessageNode> {
    items
        .iter()
        .filter_map(|item| parse_message_item(automation, item))
        .collect()
}

#[cfg(windows)]
fn parse_message_item(
    automation: &uiautomation::UIAutomation,
    item: &uiautomation::UIElement,
) -> Option<UiMessageNode> {
    if item.get_control_type().ok()? != uiautomation::controls::ControlType::ListItem {
        return None;
    }

    let runtime_id = item
        .get_runtime_id()
        .ok()?
        .into_iter()
        .map(|part| part.to_string())
        .collect::<Vec<_>>()
        .join("-");
    if runtime_id.is_empty() {
        return None;
    }

    let item_rect = item.get_bounding_rectangle().ok()?;
    let item_mid = (item_rect.get_left() + item_rect.get_right()) / 2;
    let buttons = automation
        .create_matcher()
        .from_ref(item)
        .control_type(uiautomation::controls::ControlType::Button)
        .depth(8)
        .timeout(0)
        .find_all()
        .unwrap_or_default();
    let sender_button = buttons.into_iter().find_map(|button| {
        let sender = button.get_name().ok()?.trim().to_string();
        if sender.is_empty() {
            return None;
        }
        let rect = button.get_bounding_rectangle().ok()?;
        Some((sender, rect.get_left() < item_mid))
    })?;

    let text_names = automation
        .create_matcher()
        .from_ref(item)
        .control_type(uiautomation::controls::ControlType::Text)
        .depth(8)
        .timeout(0)
        .find_all()
        .ok()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|text| text.get_name().ok())
        .collect::<Vec<_>>();
    let item_name = item.get_name().ok();
    let content = select_message_content(item_name.as_deref(), &text_names, &sender_button.0)?;

    if content.trim().is_empty() || content.trim() == sender_button.0 {
        return None;
    }

    Some(UiMessageNode {
        runtime_id,
        sender: sender_button.0,
        content,
        incoming: sender_button.1,
    })
}

#[cfg(windows)]
fn collect_ui_text(
    walker: &uiautomation::UITreeWalker,
    element: &uiautomation::UIElement,
    texts: &mut Vec<String>,
) {
    if let Ok(name) = element.get_name() {
        if !name.trim().is_empty() {
            texts.push(name);
        }
    }
    if let Ok(mut child) = walker.get_first_child(element) {
        loop {
            collect_ui_text(walker, &child, texts);
            match walker.get_next_sibling(&child) {
                Ok(next) => child = next,
                Err(_) => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_latest_sender_and_message_from_ui_text() {
        let message = extract_latest_message(&[
            "微信".to_string(),
            "张三".to_string(),
            "你好".to_string(),
            "这是最新消息".to_string(),
        ])
        .unwrap();

        assert_eq!(message.sender, "张三");
        assert_eq!(message.content, "这是最新消息");
    }

    #[test]
    fn formats_ui_scan_summary_with_message_counts() {
        let summary = UiScanSummary {
            windows: 1,
            sessions: 4,
            unread_sessions: 2,
            opened_sessions: 1,
            lists: 2,
            items: 8,
            parsed_nodes: 6,
            incoming_nodes: 3,
            text_nodes: 0,
            messages: 3,
        };

        assert_eq!(
            summary.format_log(),
            "wechat-ui windows=1 sessions=4 unread_sessions=2 opened_sessions=1 lists=2 items=8 parsed=6 incoming=3 text_nodes=0 messages=3"
        );
    }

    #[test]
    fn parses_wxauto_style_unread_session_labels() {
        assert_eq!(
            parse_unread_session_label("张三\n3条新消息"),
            Some(WeChatSession {
                name: "张三".to_string(),
                unread_count: 3,
            })
        );
        assert_eq!(
            parse_unread_session_label("工作群 12条新消息"),
            Some(WeChatSession {
                name: "工作群".to_string(),
                unread_count: 12,
            })
        );
        assert!(parse_unread_session_label("张三\n没有新消息").is_none());
    }

    #[test]
    fn parses_unread_count_from_text_child_when_item_name_is_empty() {
        let texts = vec!["李四".to_string(), "2条新消息".to_string()];
        assert_eq!(
            parse_unread_session_item(None, &texts),
            Some(WeChatSession {
                name: "李四".to_string(),
                unread_count: 2,
            })
        );
    }

    #[test]
    fn selects_only_the_latest_unread_message_items() {
        let items = ["old", "middle", "new"];
        assert_eq!(select_latest_messages(&items, 2), &["middle", "new"]);
        assert_eq!(select_latest_messages(&items, 10), &items);
        assert!(select_latest_messages(&items, 0).is_empty());
    }

    #[test]
    fn requests_main_window_fallback_when_chat_shell_has_no_message_controls() {
        assert!(should_scan_wechat_fallback(&UiScanSummary {
            windows: 1,
            sessions: 0,
            unread_sessions: 0,
            opened_sessions: 0,
            lists: 0,
            items: 0,
            parsed_nodes: 0,
            incoming_nodes: 0,
            text_nodes: 1,
            messages: 0,
        }));
        assert!(!should_scan_wechat_fallback(&UiScanSummary {
            windows: 1,
            sessions: 0,
            unread_sessions: 0,
            opened_sessions: 0,
            lists: 1,
            items: 4,
            parsed_nodes: 4,
            incoming_nodes: 2,
            text_nodes: 0,
            messages: 2,
        }));
    }

    #[test]
    fn ignores_ui_snapshots_without_a_message_body() {
        assert!(extract_latest_message(&["微信".to_string(), "张三".to_string()]).is_none());
    }

    #[test]
    fn converts_only_incoming_message_nodes() {
        let messages = messages_from_nodes(&[
            UiMessageNode {
                runtime_id: "incoming-1".to_string(),
                sender: "张三".to_string(),
                content: "你好".to_string(),
                incoming: true,
            },
            UiMessageNode {
                runtime_id: "self-1".to_string(),
                sender: "我".to_string(),
                content: "收到".to_string(),
                incoming: false,
            },
            UiMessageNode {
                runtime_id: "system-1".to_string(),
                sender: String::new(),
                content: "昨天".to_string(),
                incoming: true,
            },
        ]);

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, "wechat-ui-incoming-1");
        assert_eq!(messages[0].sender, "张三");
        assert_eq!(messages[0].content, "你好");
    }

    #[test]
    fn tracker_skips_initial_snapshot_and_emits_each_runtime_id_once() {
        let first = UiMessageNode {
            runtime_id: "message-1".to_string(),
            sender: "张三".to_string(),
            content: "第一条".to_string(),
            incoming: true,
        };
        let second = UiMessageNode {
            runtime_id: "message-2".to_string(),
            sender: "李四".to_string(),
            content: "第二条".to_string(),
            incoming: true,
        };
        let mut tracker = MessageTracker::default();

        assert!(tracker
            .ingest(messages_from_nodes(&[first.clone()]))
            .is_empty());
        let pending = tracker.ingest(messages_from_nodes(&[first, second.clone()]));
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "wechat-ui-message-2");
        tracker.mark_delivered(&pending[0].id);
        assert!(tracker.ingest(messages_from_nodes(&[second])).is_empty());
    }

    #[test]
    fn tracker_deduplicates_the_same_runtime_id_within_one_scan() {
        let node = UiMessageNode {
            runtime_id: "duplicate".to_string(),
            sender: "张三".to_string(),
            content: "重复消息".to_string(),
            incoming: true,
        };
        let baseline = messages_from_nodes(&[UiMessageNode {
            runtime_id: "baseline".to_string(),
            sender: "李四".to_string(),
            content: "历史消息".to_string(),
            incoming: true,
        }]);
        let duplicate = messages_from_nodes(&[node.clone()])[0].clone();
        let mut tracker = MessageTracker::default();
        assert!(tracker.ingest(baseline).is_empty());

        let pending = tracker.ingest(vec![duplicate.clone(), duplicate]);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "wechat-ui-duplicate");
    }

    #[test]
    fn selects_message_body_before_falling_back_to_item_name() {
        let texts = vec!["寮犱笁".to_string(), "浣犲ソ".to_string()];
        assert_eq!(
            select_message_content(Some("寮犱笁"), &texts, "寮犱笁"),
            Some("浣犲ソ".to_string())
        );
        assert_eq!(
            select_message_content(Some("浣犲ソ"), &[], "寮犱笁"),
            Some("浣犲ソ".to_string())
        );
    }
}
