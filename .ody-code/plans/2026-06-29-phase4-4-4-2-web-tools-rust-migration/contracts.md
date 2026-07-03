# Phase A — 契约与基础设施

本部分建立 Web 工具的 trait 边界与测试用的 mock provider，为后续工具实现与对照测试提供稳定契约。

---

### Task 1: 定义 `UrlFetcher`/`WebSearchProvider` trait 与错误类型

**Depends on:** none

**Files:**
- Create: `rust-ody/crates/ody-host/src/tools/web.rs`
- Modify: `rust-ody/crates/ody-host/src/tools/mod.rs` (add `pub mod web;`)

TS 源：`packages/agent-core/src/tools/builtin/web/fetch-url.ts:21-55` 与 `packages/agent-core/src/tools/builtin/web/web-search.ts:19-36`。

**步骤：**

- [ ] 在 `ody-host/src/tools/web.rs` 写入 trait 与类型：

```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 内容返回方式，对齐 TS `UrlFetchKind`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UrlFetchKind {
    Passthrough,
    Extracted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlFetchResult {
    pub content: String,
    pub kind: UrlFetchKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// 对齐 TS `HttpFetchError`：HTTP 请求已完成但状态码 ≥ 400。
#[derive(Debug)]
pub struct HttpFetchError {
    pub status: u16,
    pub message: String,
}

impl std::fmt::Display for HttpFetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HTTP {} {}", self.status, self.message)
    }
}

impl std::error::Error for HttpFetchError {}

#[derive(Debug, Clone, Default)]
pub struct UrlFetchOptions {
    pub tool_call_id: Option<String>,
}

#[async_trait]
pub trait UrlFetcher: Send + Sync {
    async fn fetch(
        &self,
        url: &str,
        options: UrlFetchOptions,
    ) -> Result<UrlFetchResult, Box<dyn std::error::Error + Send>>;
}

#[derive(Debug, Clone, Default)]
pub struct WebSearchOptions {
    pub limit: Option<u32>,
    pub include_content: bool,
    pub tool_call_id: Option<String>,
}

#[async_trait]
pub trait WebSearchProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn search(
        &self,
        query: &str,
        options: WebSearchOptions,
    ) -> Result<Vec<WebSearchResult>, Box<dyn std::error::Error + Send>>;
}
```

- [ ] 在 `ody-host/src/tools/mod.rs` 顶部 `pub mod bash;` 下方增加 `pub mod web;`。
- [ ] 写编译测试（仅验证模块能编译）：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_fetch_error_implements_error() {
        let err = HttpFetchError { status: 404, message: "Not Found".into() };
        assert_eq!(format!("{}", err), "HTTP 404 Not Found");
        assert!(std::error::Error::source(&err).is_none());
    }

    #[test]
    fn url_fetch_kind_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&UrlFetchKind::Passthrough).unwrap(), "\"passthrough\"");
        assert_eq!(serde_json::to_string(&UrlFetchKind::Extracted).unwrap(), "\"extracted\"");
    }
}
```

- [ ] 运行 `cargo test -p ody-host tools::web::tests` 验证通过。
- [ ] Commit。

---

### Task 2: 实现 mock `UrlFetcher` 与 `WebSearchProvider`

**Depends on:** Task 1

**Files:**
- Modify: `rust-ody/crates/ody-host/src/tools/web.rs`

**步骤：**

- [ ] 在 `ody-host/src/tools/web.rs` 的 trait 下方加入 mock 实现：

```rust
/// 测试用 mock fetcher。fixture 数据在构造时注入，避免真实网络。
pub struct MockUrlFetcher {
    pub result: Result<UrlFetchResult, Box<dyn std::error::Error + Send>>,
}

#[async_trait]
impl UrlFetcher for MockUrlFetcher {
    async fn fetch(
        &self,
        url: &str,
        options: UrlFetchOptions,
    ) -> Result<UrlFetchResult, Box<dyn std::error::Error + Send>> {
        let _ = (url, options); // mock 不依赖输入
        match &self.result {
            Ok(r) => Ok(r.clone()),
            Err(e) => Err(format!("{}", e).into()),
        }
    }
}

/// 测试用 mock web search provider。
pub struct MockWebSearchProvider {
    pub name: &'static str,
    pub result: Result<Vec<WebSearchResult>, Box<dyn std::error::Error + Send>>,
}

#[async_trait]
impl WebSearchProvider for MockWebSearchProvider {
    fn name(&self) -> &str {
        self.name
    }

    async fn search(
        &self,
        query: &str,
        options: WebSearchOptions,
    ) -> Result<Vec<WebSearchResult>, Box<dyn std::error::Error + Send>> {
        let _ = (query, options); // mock 不依赖输入
        match &self.result {
            Ok(r) => Ok(r.clone()),
            Err(e) => Err(format!("{}", e).into()),
        }
    }
}
```

- [ ] 写测试验证 mock 返回注入结果：

```rust
#[cfg(test)]
mod mock_tests {
    use super::*;

    #[tokio::test]
    async fn mock_fetcher_returns_injected_result() {
        let fetcher = MockUrlFetcher {
            result: Ok(UrlFetchResult {
                content: "hello".into(),
                kind: UrlFetchKind::Passthrough,
            }),
        };
        let out = fetcher.fetch("https://example.com", UrlFetchOptions::default()).await.unwrap();
        assert_eq!(out.content, "hello");
        assert_eq!(out.kind, UrlFetchKind::Passthrough);
    }

    #[tokio::test]
    async fn mock_searcher_returns_injected_results() {
        let provider = MockWebSearchProvider {
            name: "mock",
            result: Ok(vec![WebSearchResult {
                title: "t".into(),
                url: "https://example.com".into(),
                snippet: "s".into(),
                date: None,
                content: None,
            }]),
        };
        let out = provider.search("q", WebSearchOptions::default()).await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].title, "t");
    }
}
```

- [ ] 运行 `cargo test -p ody-host tools::web::mock_tests` 验证通过。
- [ ] Commit。

---

## Local Self-Review (Phase A)

- [ ] **Spec coverage**: Task 1 覆盖 trait 契约；Task 2 覆盖 mock provider（4.4.2.3 测试基础设施）。
- [ ] **Placeholder scan**: 无 TODO/TBD；所有类型字段名、枚举名与 TS 对齐。
- [ ] **No phantom tasks**: Task 1 产出可编译 trait 模块；Task 2 产出可运行 mock 测试。
- [ ] **Dependency soundness**: Task 2 仅使用 Task 1 定义的 trait/类型。
- [ ] **Caller & build soundness**: Task 1 仅新增模块并 export，未改现有签名；Task 2 未改签名。
- [ ] **Test-the-risk**: mock 返回注入结果的行为测试已覆盖；错误类型 Display 已验证。
- [ ] **Type consistency**: `UrlFetchResult.kind` 用 `"passthrough"`/`"extracted"`；`WebSearchResult` 字段名与 TS `WebSearchResult` 一致。
