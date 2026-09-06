//! 核心类型：三档决策、节点裁决、命令特征。

use std::fmt;

/// 三档分类语义（判定结果的最小单元）。
///
/// `Confirm` 在 JSON 层输出为「无意见」（不输出 decision），走 agent 正常权限提示。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Confirm,
    Deny,
}

impl Decision {
    /// JSON/协议中的字符串形态；confirm 无直接 JSON 形态（见 [`Verdict`]）。
    pub fn as_str(self) -> &'static str {
        match self {
            Decision::Allow => "allow",
            Decision::Confirm => "",
            Decision::Deny => "deny",
        }
    }

    /// 词汇单源解析（协议 `decision` 字段 / 配置 `decision` 词条共用）。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "allow" => Some(Decision::Allow),
            "confirm" => Some(Decision::Confirm),
            "deny" => Some(Decision::Deny),
            _ => None,
        }
    }
}

impl fmt::Display for Decision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Decision::Allow => "allow",
            Decision::Confirm => "confirm",
            Decision::Deny => "deny",
        })
    }
}

/// 一条简单命令的分类结果（含可选原因，用于 deny/confirm 说明）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub decision: Decision,
    pub reason: Option<String>,
}

impl Verdict {
    pub fn allow() -> Self {
        Verdict {
            decision: Decision::Allow,
            reason: None,
        }
    }

    pub fn confirm(reason: impl Into<String>) -> Self {
        Verdict {
            decision: Decision::Confirm,
            reason: Some(reason.into()),
        }
    }

    pub fn deny(reason: impl Into<String>) -> Self {
        Verdict {
            decision: Decision::Deny,
            reason: Some(reason.into()),
        }
    }

    /// 组合多条简单命令的裁决（任一 deny→deny；全 allow→allow；否则 confirm）。
    pub fn combine(verdicts: impl IntoIterator<Item = Verdict>) -> Verdict {
        let mut saw_confirm = false;
        for v in verdicts {
            match v.decision {
                Decision::Deny => return v,
                Decision::Confirm => saw_confirm = true,
                Decision::Allow => {}
            }
        }
        if saw_confirm {
            Verdict::confirm("component requires confirmation")
        } else {
            Verdict::allow()
        }
    }
}

/// 解析失败的兜底分支：安全侧 confirm，不误放行。
pub fn unparseable(reason: impl Into<String>) -> Verdict {
    Verdict::confirm(format!("unparseable: {}", reason.into()))
}
