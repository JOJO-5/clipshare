use crate::wechat::WeChatMessage;
use std::collections::{HashSet, VecDeque};
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
    tree_diagnostic: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct NativeAccessibilitySnapshot {
    child_windows: usize,
    class_names: Vec<String>,
    target_process_id: u32,
    current_process_id: u32,
    msaa_root: bool,
    msaa_nodes: usize,
    texts: Vec<String>,
    error: Option<String>,
}

fn should_use_msaa_fallback(control_descendants: usize, raw_descendants: usize) -> bool {
    control_descendants == 0 && raw_descendants == 0
}

fn format_native_accessibility_diagnostic(snapshot: &NativeAccessibilitySnapshot) -> String {
    let error = snapshot
        .error
        .as_deref()
        .map(|value| diagnostic_text(value.to_string(), 80))
        .unwrap_or_else(|| "none".to_string());
    let sample = snapshot
        .texts
        .iter()
        .take(12)
        .map(|value| diagnostic_text(value.clone(), 32))
        .collect::<Vec<_>>()
        .join("|");

    format!(
        "native_children={} native_classes={} target_pid={} current_pid={} same_process={} msaa_root={} msaa_nodes={} msaa_texts={} msaa_error={} msaa_sample={}",
        snapshot.child_windows,
        snapshot.class_names.join("|"),
        snapshot.target_process_id,
        snapshot.current_process_id,
        usize::from(snapshot.target_process_id == snapshot.current_process_id),
        usize::from(snapshot.msaa_root),
        snapshot.msaa_nodes,
        snapshot.texts.len(),
        error,
        sample,
    )
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
    if let Some(session) = candidates.iter().find_map(|candidate| {
        parse_unread_session_label(candidate)
            .or_else(|| parse_wechat4_unread_session_label(candidate))
    }) {
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

fn parse_wechat4_unread_session_label(label: &str) -> Option<WeChatSession> {
    let unread_count = label.lines().find_map(|line| {
        let marker_end = line.find("\u{6761}]")?;
        let marker_start = line[..marker_end].rfind('[')?;
        line[marker_start + 1..marker_end].parse::<usize>().ok()
    })?;
    if unread_count == 0 {
        return None;
    }

    let name = label
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !(line.contains('[') && line.contains("\u{6761}]")))?;
    Some(WeChatSession {
        name: name.to_string(),
        unread_count,
    })
}

fn parse_wechat4_message_snapshot(
    class_name: &str,
    name: &str,
    runtime_id: &str,
    sender: &str,
) -> Option<UiMessageNode> {
    if !is_wechat4_message_class(class_name)
        || name.trim().is_empty()
        || runtime_id.is_empty()
        || sender.trim().is_empty()
    {
        return None;
    }

    Some(UiMessageNode {
        runtime_id: runtime_id.to_string(),
        sender: sender.trim().to_string(),
        content: name.trim().to_string(),
        incoming: true,
    })
}

fn is_wechat4_message_class(class_name: &str) -> bool {
    matches!(
        class_name,
        "mmui::ChatTextItemView"
            | "mmui::ChatBubbleItemView"
            | "mmui::ChatVoiceItemView"
            | "mmui::ChatPersonalCardItemView"
    )
}

fn select_latest_messages<T>(items: &[T], count: usize) -> &[T] {
    let start = items.len().saturating_sub(count);
    &items[start..]
}

fn should_scan_wechat_fallback(summary: &UiScanSummary) -> bool {
    summary.parsed_nodes == 0 && summary.messages == 0 && summary.unread_sessions == 0
}

fn should_emit_tree_diagnostic(previous: Option<&str>, current: Option<&str>) -> bool {
    current.is_some() && previous != current
}

fn should_prefer_raw_view(control_descendants: usize, raw_descendants: usize) -> bool {
    control_descendants == 0 && raw_descendants > 0
}

fn should_use_handle_rebound(
    current_descendants: usize,
    rebound_descendants: usize,
    rebound_is_mmui: bool,
) -> bool {
    rebound_descendants > current_descendants || (current_descendants == 0 && rebound_is_mmui)
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
                let mut last_tree_diagnostic = None;
                let mut last_summary_log = Instant::now() - Duration::from_secs(5);
                let mut last_pending_log = Instant::now() - Duration::from_secs(5);

                while !thread_stop.load(Ordering::Relaxed) {
                    let (messages, summary) = scan_wechat(&automation);
                    if last_summary.as_ref() != Some(&summary)
                        || last_summary_log.elapsed() >= Duration::from_secs(5)
                    {
                        on_diagnostic(summary.format_log());
                        last_summary = Some(summary.clone());
                        last_summary_log = Instant::now();
                    }
                    if should_emit_tree_diagnostic(
                        last_tree_diagnostic.as_deref(),
                        summary.tree_diagnostic.as_deref(),
                    ) {
                        if let Some(diagnostic) = summary.tree_diagnostic.clone() {
                            on_diagnostic(diagnostic.clone());
                            last_tree_diagnostic = Some(diagnostic);
                        }
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
    if target.tree_diagnostic.is_none() {
        target.tree_diagnostic = source.tree_diagnostic.clone();
    }
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
    let control_walker = match automation.get_control_view_walker() {
        Ok(walker) => walker,
        Err(_) => return (Vec::new(), UiScanSummary::default()),
    };
    let raw_walker = automation.get_raw_view_walker().ok();
    let original_control_descendants = collect_ui_descendants(&control_walker, window, 1200).len();
    let original_raw_descendants = raw_walker
        .as_ref()
        .map(|walker| collect_ui_descendants(walker, window, 1200).len())
        .unwrap_or(0);
    let original_descendants = original_control_descendants.max(original_raw_descendants);
    let mut scan_window = window.clone();
    let mut rebound_attempted = false;
    let mut rebound_used = false;
    let mut rebound_descendants = 0;

    if let Ok(handle) = window.get_native_window_handle() {
        if !handle.is_invalid() {
            rebound_attempted = true;
            if let Ok(rebound) = automation.element_from_handle(handle) {
                let rebound_control_descendants =
                    collect_ui_descendants(&control_walker, &rebound, 1200).len();
                let rebound_raw_descendants = raw_walker
                    .as_ref()
                    .map(|walker| collect_ui_descendants(walker, &rebound, 1200).len())
                    .unwrap_or(0);
                rebound_descendants = rebound_control_descendants.max(rebound_raw_descendants);
                let rebound_is_mmui = rebound
                    .get_classname()
                    .map(|class_name| class_name.starts_with("mmui::"))
                    .unwrap_or(false);
                if should_use_handle_rebound(
                    original_descendants,
                    rebound_descendants,
                    rebound_is_mmui,
                ) {
                    scan_window = rebound;
                    rebound_used = true;
                }
            }
        }
    }
    let window = &scan_window;
    let control_descendants = collect_ui_descendants(&control_walker, window, 1200).len();
    let raw_descendants = raw_walker
        .as_ref()
        .map(|walker| collect_ui_descendants(walker, window, 1200).len())
        .unwrap_or(0);
    let native_snapshot = if should_use_msaa_fallback(control_descendants, raw_descendants) {
        window
            .get_native_window_handle()
            .ok()
            .filter(|handle| !handle.is_invalid())
            .map(|handle| {
                let raw: isize = handle.into();
                scan_native_accessibility(windows::Win32::Foundation::HWND(
                    raw as *mut std::ffi::c_void,
                ))
            })
    } else {
        None
    };
    let prefer_raw = should_prefer_raw_view(control_descendants, raw_descendants);
    let tree_walker = if prefer_raw {
        raw_walker.as_ref().unwrap_or(&control_walker)
    } else {
        &control_walker
    };
    let lists = find_wechat_lists(automation, tree_walker, window, prefer_raw);

    if !lists.is_empty() {
        let items = lists
            .iter()
            .flat_map(|list| tree_walker.get_children(list).unwrap_or_default())
            .collect::<Vec<_>>();
        let mut session_names = HashSet::new();
        let unread_targets = items
            .iter()
            .filter_map(|item| {
                let session =
                    parse_unread_session_item_from_element(automation, tree_walker, item)?;
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
            let current_lists = find_wechat_lists(automation, tree_walker, window, prefer_raw);
            let current_items = current_lists
                .iter()
                .flat_map(|list| tree_walker.get_children(list).unwrap_or_default())
                .collect::<Vec<_>>();
            let classic_nodes = parse_message_nodes(automation, &current_items);
            let wechat4_nodes = parse_wechat4_message_nodes(&current_items, &session.name);
            let current_nodes = if wechat4_nodes.is_empty() {
                classic_nodes
            } else {
                wechat4_nodes
            };
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
                tree_diagnostic: (nodes.is_empty() && unread_sessions == 0).then(|| {
                    format_ui_tree_diagnostic(
                        &control_walker,
                        raw_walker.as_ref(),
                        window,
                        rebound_attempted,
                        rebound_used,
                        rebound_descendants,
                        native_snapshot.as_ref(),
                    )
                }),
                ..UiScanSummary::default()
            },
        );
    }

    let mut texts = Vec::new();
    collect_ui_text(tree_walker, window, &mut texts);
    let mut messages = extract_latest_message(&texts)
        .into_iter()
        .collect::<Vec<_>>();
    if messages.is_empty() {
        if let Some(snapshot) = native_snapshot.as_ref() {
            messages.extend(extract_latest_message(&snapshot.texts));
        }
    }
    (
        messages.clone(),
        UiScanSummary {
            text_nodes: texts.len(),
            messages: messages.len(),
            tree_diagnostic: Some(format_ui_tree_diagnostic(
                &control_walker,
                raw_walker.as_ref(),
                window,
                rebound_attempted,
                rebound_used,
                rebound_descendants,
                native_snapshot.as_ref(),
            )),
            ..UiScanSummary::default()
        },
    )
}

#[cfg(windows)]
fn find_wechat_lists(
    automation: &uiautomation::UIAutomation,
    walker: &uiautomation::UITreeWalker,
    window: &uiautomation::UIElement,
    prefer_raw: bool,
) -> Vec<uiautomation::UIElement> {
    if !prefer_raw {
        let lists = automation
            .create_matcher()
            .from_ref(window)
            .control_type(uiautomation::controls::ControlType::List)
            .depth(32)
            .timeout(100)
            .find_all()
            .unwrap_or_default();
        if !lists.is_empty() {
            return lists;
        }
    }

    collect_ui_descendants(walker, window, 1200)
        .into_iter()
        .filter(|element| {
            element.get_control_type().ok() == Some(uiautomation::controls::ControlType::List)
        })
        .collect()
}

#[cfg(windows)]
fn collect_ui_descendants(
    walker: &uiautomation::UITreeWalker,
    root: &uiautomation::UIElement,
    limit: usize,
) -> Vec<uiautomation::UIElement> {
    let mut queue = VecDeque::from(walker.get_children(root).unwrap_or_default());
    let mut elements = Vec::new();

    while let Some(element) = queue.pop_front() {
        if elements.len() >= limit {
            break;
        }
        queue.extend(walker.get_children(&element).unwrap_or_default());
        elements.push(element);
    }

    elements
}

fn diagnostic_text(value: String, max_chars: usize) -> String {
    let mut text = value
        .replace('\r', " ")
        .replace('\n', " ")
        .replace('|', "/");
    if text.chars().count() > max_chars {
        text = text.chars().take(max_chars).collect();
        text.push('…');
    }
    text
}

#[cfg(windows)]
struct NativeWindowEnumeration {
    handles: Vec<windows::Win32::Foundation::HWND>,
    class_names: Vec<String>,
}

#[cfg(windows)]
unsafe extern "system" fn enumerate_native_child_window(
    hwnd: windows::Win32::Foundation::HWND,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::BOOL {
    let state = &mut *(lparam.0 as *mut NativeWindowEnumeration);
    if state.handles.len() >= 256 {
        return windows::Win32::Foundation::BOOL(0);
    }
    state.handles.push(hwnd);
    push_native_window_class(hwnd, &mut state.class_names);
    windows::Win32::Foundation::BOOL(1)
}

#[cfg(windows)]
unsafe fn push_native_window_class(
    hwnd: windows::Win32::Foundation::HWND,
    class_names: &mut Vec<String>,
) {
    use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;

    let mut buffer = [0u16; 256];
    let length = GetClassNameW(hwnd, &mut buffer);
    if length <= 0 {
        return;
    }
    let class_name = String::from_utf16_lossy(&buffer[..length as usize]);
    if !class_name.is_empty() && !class_names.contains(&class_name) {
        class_names.push(class_name);
    }
}

#[cfg(windows)]
fn scan_native_accessibility(
    root: windows::Win32::Foundation::HWND,
) -> NativeAccessibilitySnapshot {
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::UI::WindowsAndMessaging::{EnumChildWindows, GetWindowThreadProcessId};

    let mut enumeration = NativeWindowEnumeration {
        handles: Vec::new(),
        class_names: Vec::new(),
    };
    unsafe {
        push_native_window_class(root, &mut enumeration.class_names);
        let _ = EnumChildWindows(
            root,
            Some(enumerate_native_child_window),
            LPARAM(&mut enumeration as *mut NativeWindowEnumeration as isize),
        );
    }

    let mut target_process_id = 0;
    unsafe {
        GetWindowThreadProcessId(root, Some(&mut target_process_id));
    }
    let child_windows = enumeration.handles.len();
    let mut handles = Vec::with_capacity(child_windows + 1);
    handles.push(root);
    handles.extend(enumeration.handles.iter().copied());

    let mut snapshot = NativeAccessibilitySnapshot {
        child_windows,
        class_names: enumeration.class_names,
        target_process_id,
        current_process_id: std::process::id(),
        ..NativeAccessibilitySnapshot::default()
    };
    let mut text_seen = HashSet::new();
    let mut first_error = None;

    for (index, hwnd) in handles.into_iter().take(64).enumerate() {
        match accessible_from_window(hwnd) {
            Ok(accessible) => {
                if index == 0 {
                    snapshot.msaa_root = true;
                }
                collect_msaa_accessible(
                    &accessible,
                    0,
                    &mut snapshot.msaa_nodes,
                    &mut snapshot.texts,
                    &mut text_seen,
                );
                if snapshot.msaa_nodes >= 600 {
                    break;
                }
            }
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }
    if snapshot.msaa_nodes == 0 {
        snapshot.error = first_error;
    }
    snapshot
}

#[cfg(windows)]
fn accessible_from_window(
    hwnd: windows::Win32::Foundation::HWND,
) -> Result<windows::Win32::UI::Accessibility::IAccessible, String> {
    use std::ffi::c_void;
    use windows::core::Interface;
    use windows::Win32::UI::Accessibility::{AccessibleObjectFromWindow, IAccessible};

    const OBJID_CLIENT: u32 = 0xffff_fffc;
    let mut raw = std::ptr::null_mut::<c_void>();
    unsafe {
        AccessibleObjectFromWindow(hwnd, OBJID_CLIENT, &IAccessible::IID, &mut raw)
            .map_err(|error| error.to_string())?;
        if raw.is_null() {
            return Err("AccessibleObjectFromWindow returned null".to_string());
        }
        Ok(IAccessible::from_raw(raw))
    }
}

#[cfg(windows)]
fn collect_msaa_accessible(
    accessible: &windows::Win32::UI::Accessibility::IAccessible,
    depth: usize,
    node_count: &mut usize,
    texts: &mut Vec<String>,
    text_seen: &mut HashSet<String>,
) {
    use windows::core::{Interface, VARIANT};
    use windows::Win32::UI::Accessibility::IAccessible;

    if depth > 12 || *node_count >= 600 {
        return;
    }
    *node_count += 1;
    let self_id = VARIANT::from(0i32);
    collect_msaa_text(accessible, &self_id, texts, text_seen);

    let child_count = unsafe { accessible.accChildCount().unwrap_or(0) }.clamp(0, 512) as usize;
    for child_index in 1..=child_count {
        if *node_count >= 600 {
            break;
        }
        let child_id = VARIANT::from(child_index as i32);
        *node_count += 1;
        collect_msaa_text(accessible, &child_id, texts, text_seen);
        let child = unsafe { accessible.get_accChild(&child_id) }
            .ok()
            .and_then(|dispatch| dispatch.cast::<IAccessible>().ok());
        if let Some(child) = child {
            collect_msaa_accessible(&child, depth + 1, node_count, texts, text_seen);
        }
    }
}

#[cfg(windows)]
fn collect_msaa_text(
    accessible: &windows::Win32::UI::Accessibility::IAccessible,
    child_id: &windows::core::VARIANT,
    texts: &mut Vec<String>,
    text_seen: &mut HashSet<String>,
) {
    let values = unsafe {
        [
            accessible.get_accName(child_id).ok(),
            accessible.get_accValue(child_id).ok(),
            accessible.get_accDescription(child_id).ok(),
        ]
    };
    for value in values.into_iter().flatten() {
        let text = value.to_string().trim().to_string();
        if !text.is_empty() && text_seen.insert(text.clone()) && texts.len() < 200 {
            texts.push(text);
        }
    }
}

#[cfg(windows)]
fn format_ui_tree_diagnostic(
    control_walker: &uiautomation::UITreeWalker,
    raw_walker: Option<&uiautomation::UITreeWalker>,
    window: &uiautomation::UIElement,
    rebound_attempted: bool,
    rebound_used: bool,
    rebound_descendants: usize,
    native_snapshot: Option<&NativeAccessibilitySnapshot>,
) -> String {
    let control_elements = collect_ui_descendants(control_walker, window, 1200);
    let raw_elements = raw_walker
        .map(|walker| collect_ui_descendants(walker, window, 1200))
        .unwrap_or_default();
    let elements = if should_prefer_raw_view(control_elements.len(), raw_elements.len()) {
        &raw_elements
    } else {
        &control_elements
    };
    let mut lists = 0;
    let mut list_items = 0;
    let mut buttons = 0;
    let mut texts = 0;
    let mut chat_windows = 0;
    let mut mmui_elements = 0;
    let mut wechat4_message_items = 0;
    let mut sample = Vec::new();

    for element in elements.iter() {
        let control_type = element.get_control_type().ok();
        match control_type {
            Some(uiautomation::controls::ControlType::List) => lists += 1,
            Some(uiautomation::controls::ControlType::ListItem) => list_items += 1,
            Some(uiautomation::controls::ControlType::Button) => buttons += 1,
            Some(uiautomation::controls::ControlType::Text) => texts += 1,
            _ => {}
        }
        let class_name = element.get_classname().unwrap_or_default();
        if class_name == "ChatWnd" {
            chat_windows += 1;
        }
        if class_name.starts_with("mmui::") {
            mmui_elements += 1;
        }
        if is_wechat4_message_class(&class_name) {
            wechat4_message_items += 1;
        }
        if sample.len() < 24 {
            let name = element.get_name().unwrap_or_default();
            if !class_name.is_empty() || !name.is_empty() {
                sample.push(format!(
                    "{}:{}:{}",
                    format!(
                        "{:?}",
                        control_type.unwrap_or(uiautomation::controls::ControlType::Custom)
                    ),
                    diagnostic_text(class_name, 24),
                    diagnostic_text(name, 32),
                ));
            }
        }
    }

    let mut diagnostic = format!(
        "wechat-ui-tree root_class={} root_name={} control_descendants={} raw_descendants={} descendants={} handle_rebound_attempted={} handle_rebound_used={} rebound_descendants={} lists={} list_items={} buttons={} texts={} chatwnd={} mmui={} wechat4_items={} sample={}",
        diagnostic_text(window.get_classname().unwrap_or_default(), 40),
        diagnostic_text(window.get_name().unwrap_or_default(), 40),
        control_elements.len(),
        raw_elements.len(),
        elements.len(),
        usize::from(rebound_attempted),
        usize::from(rebound_used),
        rebound_descendants,
        lists,
        list_items,
        buttons,
        texts,
        chat_windows,
        mmui_elements,
        wechat4_message_items,
        sample.join("|")
    );
    if let Some(snapshot) = native_snapshot {
        diagnostic.push(' ');
        diagnostic.push_str(&format_native_accessibility_diagnostic(snapshot));
    }
    diagnostic
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
fn parse_wechat4_message_nodes(
    items: &[uiautomation::UIElement],
    sender: &str,
) -> Vec<UiMessageNode> {
    items
        .iter()
        .filter_map(|item| {
            let runtime_id = item
                .get_runtime_id()
                .ok()?
                .into_iter()
                .map(|part| part.to_string())
                .collect::<Vec<_>>()
                .join("-");
            parse_wechat4_message_snapshot(
                &item.get_classname().ok()?,
                &item.get_name().ok()?,
                &runtime_id,
                sender,
            )
        })
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
            tree_diagnostic: None,
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
    fn parses_wechat4_unread_session_labels() {
        assert_eq!(
            parse_wechat4_unread_session_label("\u{5f20}\u{4e09}\n[3\u{6761}]\n\u{4f60}\u{597d}"),
            Some(WeChatSession {
                name: "\u{5f20}\u{4e09}".to_string(),
                unread_count: 3,
            })
        );
        assert!(parse_wechat4_unread_session_label("\u{5f20}\u{4e09}\n\u{4f60}\u{597d}").is_none());
    }

    #[test]
    fn parses_wechat4_message_items_from_mmui_snapshot() {
        assert_eq!(
            parse_wechat4_message_snapshot(
                "mmui::ChatTextItemView",
                "\u{4f60}\u{597d}",
                "42-7",
                "\u{5f20}\u{4e09}",
            ),
            Some(UiMessageNode {
                runtime_id: "42-7".to_string(),
                sender: "\u{5f20}\u{4e09}".to_string(),
                content: "\u{4f60}\u{597d}".to_string(),
                incoming: true,
            })
        );
        assert!(parse_wechat4_message_snapshot(
            "mmui::ChatSessionItemView",
            "\u{5f20}\u{4e09}",
            "42-8",
            "\u{5f20}\u{4e09}",
        )
        .is_none());
        assert!(parse_wechat4_message_snapshot(
            "mmui::ChatSystemItemView",
            "10:20",
            "42-9",
            "\u{5f20}\u{4e09}",
        )
        .is_none());
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
            tree_diagnostic: None,
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
            tree_diagnostic: None,
        }));
    }

    #[test]
    fn emits_tree_diagnostic_only_when_the_snapshot_changes() {
        assert!(should_emit_tree_diagnostic(None, Some("tree-a")));
        assert!(!should_emit_tree_diagnostic(Some("tree-a"), Some("tree-a")));
        assert!(should_emit_tree_diagnostic(Some("tree-a"), Some("tree-b")));
        assert!(!should_emit_tree_diagnostic(Some("tree-a"), None));
    }

    #[test]
    fn prefers_raw_ui_tree_when_control_view_is_empty() {
        assert!(should_prefer_raw_view(0, 4));
        assert!(!should_prefer_raw_view(4, 0));
        assert!(!should_prefer_raw_view(0, 0));
    }

    #[test]
    fn prefers_handle_rebound_element_only_when_it_exposes_more_ui() {
        assert!(should_use_handle_rebound(0, 12, false));
        assert!(should_use_handle_rebound(0, 0, true));
        assert!(!should_use_handle_rebound(8, 2, false));
        assert!(!should_use_handle_rebound(0, 0, false));
    }

    #[test]
    fn uses_msaa_fallback_only_when_uia_exposes_no_descendants() {
        assert!(should_use_msaa_fallback(0, 0));
        assert!(!should_use_msaa_fallback(1, 0));
        assert!(!should_use_msaa_fallback(0, 1));
    }

    #[test]
    fn formats_native_accessibility_diagnostic_for_remote_debugging() {
        let snapshot = NativeAccessibilitySnapshot {
            child_windows: 3,
            class_names: vec![
                "Qt51514QWindowIcon".to_string(),
                "WeChatMainWndForPC".to_string(),
            ],
            target_process_id: 4242,
            current_process_id: 3131,
            msaa_root: true,
            msaa_nodes: 12,
            texts: vec!["张三".to_string(), "你好".to_string()],
            error: Some("access denied".to_string()),
        };

        assert_eq!(
            format_native_accessibility_diagnostic(&snapshot),
            "native_children=3 native_classes=Qt51514QWindowIcon|WeChatMainWndForPC target_pid=4242 current_pid=3131 same_process=0 msaa_root=1 msaa_nodes=12 msaa_texts=2 msaa_error=access denied msaa_sample=张三|你好"
        );
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
