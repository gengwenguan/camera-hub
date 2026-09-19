//! Conservative whole-utterance rules. Unconsumed text rejects the entire
//! request; no network, device state, or model inference is involved here.
use crate::ir::{AcMode, AirconCommand};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParseState {
    Ready,
    Rejected,
    Ignored,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ParseResult {
    pub state: ParseState,
    pub intent: Option<AirconCommand>,
    pub message: String,
}

impl ParseResult {
    fn rejected(message: impl Into<String>) -> Self {
        Self {
            state: ParseState::Rejected,
            intent: None,
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    #[default]
    Idle,
    Listening,
    Preview,
    Disabled,
    Rejected,
    Ignored,
    Cooldown,
    Executing,
    Sent,
    Failed,
}

pub fn parse(text: &str) -> ParseResult {
    if text.chars().count() > 160 {
        return ParseResult::rejected("指令太长，请一次说一个空调操作");
    }
    let normalized = text
        .to_lowercase()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    if normalized.contains(['不', '别', '勿', '吗', '么', '？', '?'])
        || [
            "是否",
            "能否",
            "可否",
            "可以",
            "为什么",
            "怎么",
            "假如",
            "如果",
            "要是",
            "还是",
            "或者",
        ]
        .iter()
        .any(|word| normalized.contains(word))
    {
        return ParseResult::rejected("否定、询问或条件句不会执行，请直接说要执行的操作");
    }
    let mut input = normalized.trim_matches(['，', ',', '。', '.', '！', '!']);
    // Polite preamble only, never discard arbitrary words inside a command.
    for _ in 0..6 {
        if let Some(rest) = strip_any(
            input,
            &[
                "小雨",
                "麻烦你",
                "麻烦",
                "请你",
                "请",
                "帮我",
                "给我",
                "把",
                "将",
            ],
        ) {
            input = rest.trim_start_matches(['，', ',']);
        } else {
            break;
        }
    }
    if input.is_empty() {
        return ParseResult {
            state: ParseState::Ignored,
            intent: None,
            message: "等待空调指令".to_owned(),
        };
    }
    if input.matches("空调").count() != 1 {
        return ParseResult::rejected("请明确说一个空调操作，例如：把空调调到二十度");
    }
    // A single explicit device may occur before or after the operation:
    // 打开空调 / 空调打开 / 把空调调到20度 / 关闭空调的ECO.
    let input = input.replacen("空调的", "", 1).replacen("空调", "", 1);
    let mut rest = input.as_str();
    let mut power = None;
    let mut temperature = None;
    let mut mode = None;
    let mut eco = None;
    let mut has_value = false;
    let mut awaiting_value = false;

    while !rest.is_empty() {
        if let Some(next) = strip_any(rest, &["并且", "然后", "同时", "并", "再", "和", "，", ","])
        {
            if !has_value || awaiting_value {
                return ParseResult::rejected("连接词前后需要完整的空调操作");
            }
            awaiting_value = true;
            rest = next;
            continue;
        }
        if let Some(next) = strip_any(
            rest,
            &[
                "温度设置为",
                "温度调整到",
                "温度调到",
                "温度设为",
                "模式设置为",
                "模式调到",
                "模式设为",
                "设置为",
                "调整到",
                "调节到",
                "切换到",
                "切换为",
                "调成",
                "调至",
                "调到",
                "设为",
                "设到",
                "设置",
                "温度",
                "到",
            ],
        ) {
            // Only one setting verb per value (e.g. reject "调到调到20度").
            if awaiting_value && !has_value {
                return ParseResult::rejected("请说完整的温度或模式");
            }
            awaiting_value = true;
            has_value = false;
            rest = next;
            continue;
        }

        let mut matched = false;
        for (words, enabled) in [
            (
                &[
                    "关闭eco模式",
                    "关闭eco",
                    "关掉eco",
                    "关eco",
                    "取消eco",
                    "eco关闭",
                    "关闭节能模式",
                    "关节能模式",
                    "关节能",
                    "退出节能模式",
                ][..],
                false,
            ),
            (
                &[
                    "开启eco模式",
                    "打开eco模式",
                    "开启eco",
                    "打开eco",
                    "开eco",
                    "eco开启",
                    "开启节能模式",
                    "打开节能模式",
                    "开节能模式",
                    "节能模式",
                    "eco模式",
                    "eco",
                ][..],
                true,
            ),
        ] {
            if let Some(next) = strip_any(rest, words) {
                if eco.replace(enabled).is_some() {
                    return ParseResult::rejected("ECO 操作重复或冲突");
                }
                rest = next;
                matched = true;
                break;
            }
        }
        if !matched {
            for (word, value) in [
                ("制冷", AcMode::Cool),
                ("抽湿", AcMode::Dry),
                ("除湿", AcMode::Dry),
                ("制热", AcMode::Heat),
                ("自动", AcMode::Auto),
            ] {
                if let Some(next) = rest.strip_prefix(word) {
                    if mode.replace(value).is_some() {
                        return ParseResult::rejected("一次只能设置一种空调模式");
                    }
                    rest = next.strip_prefix("模式").unwrap_or(next);
                    matched = true;
                    break;
                }
            }
        }
        if !matched {
            for (words, enabled) in [
                (&["打开", "开启", "开机", "开"][..], true),
                (&["关闭", "关掉", "关机", "关"][..], false),
            ] {
                if let Some(next) = strip_any(rest, words) {
                    if power.replace(enabled).is_some() {
                        return ParseResult::rejected("开关操作重复或冲突");
                    }
                    rest = next;
                    matched = true;
                    break;
                }
            }
        }
        if !matched {
            let end = rest
                .char_indices()
                .find(|(_, ch)| !ch.is_ascii_digit() && !"零〇一二两三四五六七八九十".contains(*ch))
                .map(|(index, _)| index)
                .unwrap_or(rest.len());
            if end > 0 {
                let number = &rest[..end];
                let Some(value) = integer(number) else {
                    return ParseResult::rejected("请使用明确的整数温度，例如二十度");
                };
                let Some(next) = strip_any(&rest[end..], &["摄氏度", "°c", "℃", "度"])
                else {
                    return ParseResult::rejected("温度需要带“度”，仅支持 17–30°C 的整数");
                };
                if temperature.replace(value).is_some() {
                    return ParseResult::rejected("一次只能设置一个明确温度");
                }
                rest = next;
                matched = true;
            }
        }
        if !matched {
            return ParseResult::rejected(
                "没有完整理解指令；暂不支持模糊温度、半度、调高调低、风速或定时",
            );
        }
        has_value = true;
        awaiting_value = false;
    }
    if awaiting_value || !has_value {
        return ParseResult::rejected("指令不完整，请说要设置的温度、模式或开关操作");
    }
    let command = if power == Some(false) {
        if temperature.is_some() || mode.is_some() || eco.is_some() {
            return ParseResult::rejected("关机不能同时设置温度、模式或 ECO");
        }
        AirconCommand::Off {}
    } else if power.is_none() && temperature.is_none() && mode.is_none() {
        let Some(enabled) = eco else {
            return ParseResult::rejected("没有识别到空调操作");
        };
        AirconCommand::Eco { enabled }
    } else {
        let bare_open = power == Some(true) && temperature.is_none() && mode.is_none();
        AirconCommand::Set {
            temperature_c: temperature.unwrap_or(26),
            mode: mode.unwrap_or(AcMode::Cool),
            eco: eco.unwrap_or(bare_open),
        }
    };
    if let Err(error) = command.validate() {
        return ParseResult::rejected(error.to_string());
    }
    ParseResult {
        state: ParseState::Ready,
        message: command.label(),
        intent: Some(command),
    }
}

fn strip_any<'a>(input: &'a str, words: &[&str]) -> Option<&'a str> {
    words.iter().find_map(|word| input.strip_prefix(word))
}

fn integer(text: &str) -> Option<u8> {
    if text.bytes().all(|byte| byte.is_ascii_digit()) {
        return text.parse().ok();
    }
    fn digit(ch: char) -> Option<u8> {
        match ch {
            '一' => Some(1),
            '二' | '两' => Some(2),
            '三' => Some(3),
            '四' => Some(4),
            '五' => Some(5),
            '六' => Some(6),
            '七' => Some(7),
            '八' => Some(8),
            '九' => Some(9),
            _ => None,
        }
    }
    let chars = text.chars().collect::<Vec<_>>();
    match chars.as_slice() {
        ['十'] => Some(10),
        ['十', ones] => Some(10 + digit(*ones)?),
        [tens, '十'] => Some(digit(*tens)? * 10),
        [tens, '十', ones] => Some(digit(*tens)? * 10 + digit(*ones)?),
        [one] => digit(*one),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(temperature_c: u8, mode: AcMode, eco: bool) -> AirconCommand {
        AirconCommand::Set {
            temperature_c,
            mode,
            eco,
        }
    }

    #[test]
    fn parses_complete_temperature_requests_with_explicit_defaults() {
        for text in [
            "小雨把空调调到二十度",
            "请帮我把空调调到20度。",
            "小雨，打开空调到二十度",
            "空调温度设置为20摄氏度",
            "将空调设为20℃",
            "空调20°C",
            "空调调到二十度制冷",
        ] {
            let result = parse(text);
            assert_eq!(
                result.state,
                ParseState::Ready,
                "{text}: {}",
                result.message
            );
            assert_eq!(result.intent, Some(set(20, AcMode::Cool, false)), "{text}");
        }
    }

    #[test]
    fn parses_all_chinese_temperature_steps() {
        for (index, number) in [
            "十七",
            "十八",
            "十九",
            "二十",
            "二十一",
            "二十二",
            "二十三",
            "二十四",
            "二十五",
            "二十六",
            "二十七",
            "二十八",
            "二十九",
            "三十",
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                parse(&format!("空调{number}度")).intent,
                Some(set(index as u8 + 17, AcMode::Cool, false))
            );
            assert_eq!(
                parse(&format!("空调{}度", index + 17)).intent,
                Some(set(index as u8 + 17, AcMode::Cool, false))
            );
        }
    }

    #[test]
    fn supports_modes_power_and_explicit_eco() {
        for (text, expected) in [
            ("小雨打开空调", set(26, AcMode::Cool, true)),
            ("小雨空调制冷", set(26, AcMode::Cool, false)),
            ("空调切换到除湿模式", set(26, AcMode::Dry, false)),
            ("空调抽湿二十五度", set(25, AcMode::Dry, false)),
            ("空调制热三十度", set(30, AcMode::Heat, false)),
            ("空调自动模式", set(26, AcMode::Auto, false)),
            ("开启空调并调到20度然后开启ECO", set(20, AcMode::Cool, true)),
            ("空调关机", AirconCommand::Off {}),
            ("小雨关闭空调", AirconCommand::Off {}),
            ("开启空调的节能模式", AirconCommand::Eco { enabled: true }),
            ("空调开ECO", AirconCommand::Eco { enabled: true }),
            ("空调关闭ECO", AirconCommand::Eco { enabled: false }),
            ("空调关ECO", AirconCommand::Eco { enabled: false }),
            ("空调退出节能模式", AirconCommand::Eco { enabled: false }),
        ] {
            let result = parse(text);
            assert_eq!(result.intent, Some(expected), "{text}: {}", result.message);
        }
    }

    #[test]
    fn rejects_entire_ambiguous_conflicting_or_unsupported_utterance() {
        for text in [
            "不要打开空调",
            "小雨别把空调调到20度",
            "打开空调吗",
            "空调20度？",
            "如果热就打开空调",
            "能否关闭空调",
            "把空调调到二十多度",
            "空调20度左右",
            "空调二十点五度",
            "空调20.5度",
            "空调20度半",
            "空调十六度",
            "空调三十一度",
            "空调0度",
            "空调99999999999999度",
            "空调20度到26度",
            "空调20度或26度",
            "空调制冷制热",
            "关闭空调并开机",
            "关闭空调到20度",
            "空调抽湿开启ECO",
            "空调制热节能模式",
            "空调开ECO关ECO",
            "空调定时两小时",
            "空调20度两小时后关机",
            "空调20度并开灯",
            "打开空调并关闭电视",
            "空调风速三档",
            "空调20度高风",
            "空调温度调低",
            "空调调到",
            "空调",
            "空调20",
            "空调20度然后",
            "空调并打开",
            "空调调到调到20度",
            "把电视调到20度",
            "小雨二十度",
            "开空调关空调",
            "空调二二度",
            "空调20度<script>",
        ] {
            let result = parse(text);
            assert_eq!(result.state, ParseState::Rejected, "{text}: {result:?}");
            assert_eq!(result.intent, None, "{text}");
        }
    }

    #[test]
    fn ignores_empty_and_wake_only_input() {
        for text in ["", " ", "小雨", "小雨。", "小雨，小雨"] {
            assert_eq!(parse(text).state, ParseState::Ignored, "{text}");
            assert!(parse(text).intent.is_none());
        }
        assert_eq!(parse(&"空调".repeat(81)).state, ParseState::Rejected);
    }
}
