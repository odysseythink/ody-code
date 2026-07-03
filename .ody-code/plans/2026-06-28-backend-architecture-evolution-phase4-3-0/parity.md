# Part 6: L4 Parity — TS↔Rust records 交叉读写 golden fixtures

本部分是 4.3.0 的收官门：用固定 fixture 验证 TS 写出的 WAL 文件能被 Rust 完整读取并迁移，同时 Rust 写出的 WAL 文件能被 TS 完整读取。任何字段名、顺序、`blobref:` 协议、版本迁移的差异都会在此暴露。

---

### Task 1: Golden fixtures 与生成脚本

**Depends on:** `records.md` Task 3 / `blobstore.md` Task 3

**Files:**
- Create: `rust-ody/crates/agent-rs/tests/fixtures/ts-written/records.jsonl`
- Create: `rust-ody/crates/agent-rs/tests/fixtures/ts-written/blobs/...`
- Create: `rust-ody/crates/agent-rs/tests/fixtures/v1.0/records.jsonl`
- Create: `scripts/generate-record-fixtures.ts`
- Modify: `package.json` root（可选：新增 `scripts/generate-record-fixtures`）

**目标：** 构造三份 fixture：
1. `ts-written/`：由 TS 的 `BlobStore` + `FileSystemAgentRecordPersistence` 写出的当前版本记录（含 data URI 图片被卸载成 blobref）。
2. `v1.0/`：手写旧格式 JSONL，包含嵌套 `function` 的 tool call，用于验证 Rust 迁移链。
3. 为 Rust 反向写出 fixture 预留 `rust-written/` 目录（由 Task 3 填充）。

- [ ] 创建 `scripts/generate-record-fixtures.ts`：

```ts
import { mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';

import {
  BlobStore,
  FileSystemAgentRecordPersistence,
} from '../packages/agent-core/src/agent/records/index.js';
import type { AgentRecord } from '../packages/agent-core/src/agent/records/types.js';
import { ContentPart } from '../packages/kosong/src/message.js';

const outDir = join(import.meta.dirname, '..', 'rust-ody/crates/agent-rs/tests/fixtures/ts-written');
rmSync(outDir, { recursive: true, force: true });
mkdirSync(join(outDir, 'blobs'), { recursive: true });

const blobsDir = join(outDir, 'blobs');
const blobStore = new BlobStore({ blobsDir, threshold: 1 });
const persistence = new FileSystemAgentRecordPersistence(join(outDir, 'records.jsonl'), {
  blobStore,
});

const imageDataUrl = `data:image/png;base64,${'R0lGODlhAQABAIAAAAUEBAAAACwAAAAAAQABAAACAkQBADs='}`;

const records: AgentRecord[] = [
  {
    type: 'metadata',
    protocol_version: '1.3',
    created_at: 1700000000000,
    app_version: '0.0.0',
  },
  {
    type: 'turn.prompt',
    time: 1700000000001,
    input: [
      ContentPart.text({ text: 'hello' }),
      ContentPart.imageUrl({ imageUrl: { url: imageDataUrl } }),
    ],
    origin: { kind: 'user' },
  },
  {
    type: 'context.append_message',
    time: 1700000000002,
    message: {
      role: 'assistant',
      content: [ContentPart.text({ text: 'ok' })],
      toolCalls: [],
      toolCallId: undefined,
      name: undefined,
      partial: undefined,
    },
  },
];

for (const record of records) {
  persistence.append(record);
}
await persistence.flush();
await persistence.close();

console.log(`Wrote ${outDir}`);
```

> 说明：`ContentPart.text` / `ContentPart.imageUrl` 需使用 `kosong` 实际 API；若命名不同，按现有 API 调整。fixture 生成脚本只会在 fixture 过期时手动重跑，不会进入常规测试路径。

- [ ] 运行生成脚本：

```bash
node --experimental-strip-types scripts/generate-record-fixtures.ts
```

预期输出：`Wrote /Users/ranwei/workspace/ody-code/rust-ody/crates/agent-rs/tests/fixtures/ts-written`，且目录下出现 `records.jsonl` 与 `blobs/<sha256>`。

- [ ] 手动创建 `rust-ody/crates/agent-rs/tests/fixtures/v1.0/records.jsonl`：

```jsonl
{"type":"metadata","protocol_version":"1.0","created_at":1700000000000,"app_version":"0.0.0"}
{"type":"context.append_message","time":1,"message":{"role":"assistant","content":[],"toolCalls":[{"type":"function","id":"call_1","function":{"name":"read","arguments":"{}"}}]}}
```

- [ ] Commit：`test(agent-rs): add TS-written and v1.0 wire record fixtures`

---

### Task 2: Rust 读取 TS fixture 与 v1.0 fixture

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/agent-rs/tests/fixture_parity.rs`

**目标：** 在 Rust 侧加载 TS 写出的 fixture，验证反序列化、water化、字段值与 TS 一致；加载 v1.0 fixture，验证迁移链能正确升级到 v1.3。

- [ ] 创建 `rust-ody/crates/agent-rs/tests/fixture_parity.rs`：

```rust
use std::path::PathBuf;

use agent_rs::records::blobstore::{BlobStore, BlobStoreOptions};
use agent_rs::records::persistence::{AgentRecordPersistence, FileSystemAgentRecordPersistence, FileSystemAgentRecordPersistenceOptions};
use agent_rs::records::records::{AgentRecords, RecordRestoreTarget, ReplayResult};
use agent_rs::records::types::AgentRecord;
use futures_util::TryStreamExt;

struct CollectingTarget(Vec<AgentRecord>);

impl RecordRestoreTarget for CollectingTarget {
    fn restore_record(&mut self, record: &AgentRecord) {
        self.0.push(record.clone());
    }
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[tokio::test]
async fn rust_reads_ts_written_fixture_with_blobref() {
    let dir = fixture_path("ts-written");
    let records_path = dir.join("records.jsonl");
    let blobs_dir = dir.join("blobs");

    let blob_store = std::sync::Arc::new(BlobStore::new(BlobStoreOptions {
        blobs_dir,
        threshold: 1,
        max_cache_size: None,
    }));
    let options = FileSystemAgentRecordPersistenceOptions {
        blob_store: Some(blob_store.clone()),
        ..Default::default()
    };
    let persistence = FileSystemAgentRecordPersistence::with_options(&records_path, options);
    let mut records = AgentRecords::new(CollectingTarget(Vec::new()), "0.0.0", Some(Box::new(persistence)));
    let result = records.replay().await.unwrap();

    assert!(result.warning.is_none());
    assert_eq!(result.records.len(), 3);

    // Metadata
    assert!(matches!(result.records[0], AgentRecord::Metadata { .. }));

    // Turn prompt: the image data URI should have been offloaded to blobref by TS.
    match &result.records[1] {
        AgentRecord::TurnPrompt { input, .. } => {
            assert_eq!(input.len(), 2);
            match &input[1] {
                kosong_rs::message::ContentPart::ImageUrl { image_url } => {
                    assert!(image_url.url.starts_with("blobref:image/png;"));
                }
                _ => panic!("expected image_url part"),
            }
        }
        _ => panic!("expected turn.prompt"),
    }
}

#[tokio::test]
async fn rust_migrates_v1_0_fixture() {
    let dir = fixture_path("v1.0");
    let records_path = dir.join("records.jsonl");

    let persistence = FileSystemAgentRecordPersistence::new(&records_path);
    let mut records = AgentRecords::new(CollectingTarget(Vec::new()), "0.0.0", Some(Box::new(persistence)));
    let result = records.replay().await.unwrap();

    assert!(result.warning.is_none());
    assert_eq!(result.records.len(), 2);

    match &result.records[1] {
        AgentRecord::ContextAppendMessage { message, .. } => {
            assert_eq!(message.tool_calls.len(), 1);
            assert_eq!(message.tool_calls[0].name, "read");
            assert_eq!(message.tool_calls[0].arguments.as_deref(), Some("{}"));
        }
        _ => panic!("expected context.append_message"),
    }
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p agent-rs fixture_parity --test fixture_parity
```

预期输出：`test result: ok. 2 passed; 0 failed`。

- [ ] Commit：`test(agent-rs): verify Rust can read TS-written WAL fixtures`

---

### Task 3: TS 读取 Rust-written fixture

**Depends on:** Task 2

**Files:**
- Create: `rust-ody/crates/agent-rs/src/bin/generate_fixtures.rs`
- Modify: `rust-ody/crates/agent-rs/Cargo.toml`（追加 `[[bin]]`）
- Create: `packages/agent-core/src/agent/records/records.parity.test.ts`

**目标：** 让 Rust 用自身实现写出一套 fixture，再用 TS 的 `FileSystemAgentRecordPersistence` + `BlobStore` 读取，验证字段一致、blobref 能水化回 data URI。

- [ ] 创建 `rust-ody/crates/agent-rs/src/bin/generate_fixtures.rs`：

```rust
use std::env;
use std::path::PathBuf;

use agent_rs::records::blobstore::{BlobStore, BlobStoreOptions};
use agent_rs::records::nested::PromptOrigin;
use agent_rs::records::persistence::{AgentRecordPersistence, FileSystemAgentRecordPersistence, FileSystemAgentRecordPersistenceOptions};
use agent_rs::records::types::AgentRecord;
use kosong_rs::message::{ContentPart, UrlPayload};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let out_dir = env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("rust-written"));
    let blobs_dir = out_dir.join("blobs");
    std::fs::create_dir_all(&blobs_dir)?;

    let blob_store = std::sync::Arc::new(BlobStore::new(BlobStoreOptions {
        blobs_dir: blobs_dir.clone(),
        threshold: 1,
        max_cache_size: None,
    }));
    let options = FileSystemAgentRecordPersistenceOptions {
        blob_store: Some(blob_store),
        ..Default::default()
    };
    let mut persistence = FileSystemAgentRecordPersistence::with_options(&out_dir.join("records.jsonl"), options);

    let image_data_url = "data:image/png;base64,R0lGODlhAQABAIAAAAUEBAAAACwAAAAAAQABAAACAkQBADs=";

    persistence.append(AgentRecord::Metadata {
        time: Some(1700000000000),
        protocol_version: "1.3".into(),
        created_at: 1700000000000,
        app_version: Some("0.0.0".into()),
        resumed: None,
    });
    persistence.append(AgentRecord::TurnPrompt {
        time: Some(1700000000001),
        input: vec![
            ContentPart::Text { text: "hello".into() },
            ContentPart::ImageUrl {
                image_url: UrlPayload { url: image_data_url.into(), id: None },
            },
        ],
        origin: PromptOrigin::User,
    });
    persistence.append(AgentRecord::ContextAppendMessage {
        time: Some(1700000000002),
        message: agent_rs::records::nested::ContextMessage {
            message: kosong_rs::message::Message::assistant(vec![ContentPart::Text { text: "ok".into() }], vec![]),
            origin: None,
            is_error: None,
        },
    });

    persistence.flush().await?;
    persistence.close().await?;

    println!("Wrote Rust fixtures to {}", out_dir.display());
    Ok(())
}
```

- [ ] 在 `rust-ody/crates/agent-rs/Cargo.toml` 追加 bin 入口：

```toml
[[bin]]
name = "generate-fixtures"
path = "src/bin/generate_fixtures.rs"
```

- [ ] 生成 Rust fixture 到测试目录：

```bash
cd rust-ody && cargo run -p agent-rs --bin generate-fixtures -- ../crates/agent-rs/tests/fixtures/rust-written
```

预期输出：`Wrote Rust fixtures to ...rust-written`，目录下出现 `records.jsonl` 与 `blobs/<sha256>`。

- [ ] 创建 TS vitest 测试 `packages/agent-core/src/agent/records/records.parity.test.ts`：

```ts
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import {
  BlobStore,
  FileSystemAgentRecordPersistence,
} from './index.js';

describe('records parity: TS reads Rust-written WAL', () => {
  const fixtureDir = join(import.meta.dirname, '..', '..', '..', '..', 'rust-ody/crates/agent-rs/tests/fixtures/rust-written');
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = mkdtempSync(join(tmpdir(), 'records-parity-'));
  });

  afterEach(() => {
    rmSync(tmpDir, { recursive: true, force: true });
  });

  it('reads metadata, turn.prompt, and context.append_message', async () => {
    const persistence = new FileSystemAgentRecordPersistence(join(fixtureDir, 'records.jsonl'), {
      blobStore: new BlobStore({ blobsDir: join(fixtureDir, 'blobs'), threshold: 1 }),
    });

    const records: unknown[] = [];
    for await (const record of persistence.read()) {
      records.push(record);
    }

    expect(records).toHaveLength(3);
    expect(records[0]).toMatchObject({ type: 'metadata', protocol_version: '1.3' });
    expect(records[1]).toMatchObject({ type: 'turn.prompt' });
    expect(records[2]).toMatchObject({ type: 'context.append_message' });
  });

  it('rehydrates blobref back to original data URI', async () => {
    const persistence = new FileSystemAgentRecordPersistence(join(fixtureDir, 'records.jsonl'), {
      blobStore: new BlobStore({ blobsDir: join(fixtureDir, 'blobs'), threshold: 1 }),
    });

    const records: unknown[] = [];
    for await (const record of persistence.read()) {
      records.push(record);
    }

    const prompt = records[1] as { input: Array<{ imageUrl?: { url: string } }> };
    const imagePart = prompt.input.find((p) => 'imageUrl' in p);
    expect(imagePart).toBeDefined();
    expect(imagePart!.imageUrl!.url).toMatch(/^data:image\/png;base64,/);
  });
});
```

- [ ] 运行 TS 对照测试：

```bash
cd packages/agent-core && pnpm vitest run src/agent/records/records.parity.test.ts
```

预期输出：`Test Files 1 passed (1)`。

- [ ] Commit：`test(agent-core): add TS-Rust records cross-read parity test`

---

## Local Self-Review

- [ ] 1. Spec-coverage：本部分覆盖 Roadmap 4.3.0.7（L4 records 互读 fixture）。
- [ ] 2. Placeholder scan：无 TODO/TBD；fixture 路径、生成脚本、测试断言全部给出。
- [ ] 3. No phantom tasks：Task 1 产出 fixture 文件；Task 2 产出 Rust 读 TS fixture 测试；Task 3 产出 Rust 写 fixture 工具与 TS 读测试。
- [ ] 4. Dependency soundness：Task 1 依赖 records/blobstore 实现；Task 2 依赖 Task 1；Task 3 依赖 Task 2。无反向依赖。
- [ ] 5. Caller & build soundness：新增 `generate-fixtures` bin 需要在 `Cargo.toml` 注册；新增 TS test 文件不影响其他调用方；结束时分别运行 `cargo check -p agent-rs --workspace --tests` 与 `pnpm -r typecheck`（根目录）验证整树。
- [ ] 6. Test-the-risk：`rust_reads_ts_written_fixture_with_blobref` 断言 TS 卸载后的 `blobref:` URL 被 Rust 正确识别；`rust_migrates_v1_0_fixture` 断言旧格式 tool call 被迁移成当前扁平格式；TS 测试断言 Rust 写出的 `blobref` 能被水化回 `data:` URL。
- [ ] 7. Type一致性：fixture 中 `protocol_version` 统一为 `"1.3"`；`BlobStore` 的 `blobref:<mime>;<sha256>` 格式在两侧一致；data URI payload 使用同一 base64 字符串，确保哈希去重跨语言等价。
