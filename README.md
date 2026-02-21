# Freight Data Server
> **Mục tiêu:** Tách traffic data/asset ra khỏi game logic server, chuyển sang Freight (Rust/tokio) để giảm tải băng thông cho game server chính.

---

### Kiến trúc tổng quan

```
 ┌────────────────────────┐
 │      Unity Client       │
 │                         │
 │  Session_ME ──────────► │──── TCP ────► Game Logic Server (Java)
 │  Session_ME2 ─────────► │──── TCP ────► Game Logic Server2
 │                         │
 │  Session_Freight ─────► │──── TCP ────► Freight Data Server (Rust)
 │         ▲               │                  │
 │   Service_Freight       │          ┌───────┴────────┐
 │   Service_Freight2      │          │  config.yml    │
 │         │               │          │  ./data/       │
 │   FreightController     │          │   x1/icon/     │
 │         │               │          │   x2/icon/     │
 │   Controller.onMessage()│          │   x4/icon/     │
 └────────────────────────┘          └────────────────┘
```

### Luồng kết nối Freight

```
Client                              Freight Server
  │                                       │
  │─── TCP Connect ──────────────────────►│
  │─── GET_SESSION_ID (-27) ────────────►│
  │◄── Key Exchange (XOR key) ──────────│
  │─── FREIGHT_INIT (1) {zoom,w,h} ───►│  ← mới
  │◄── ACK {accepted_zoom} ────────────│  ← mới
  │                                       │
  │─── REQUEST_ICON (-67) {id} ────────►│
  │◄── icon binary data ────────────────│
  │       (file: ./data/x{zoom}/icon/{id}.png)
  │                                       │
  │─── GET_EFFDATA (-66) {id} ─────────►│
  │◄── effect binary data ──────────────│
  │       (file: ./data/x{zoom}/effectdata/{id}.dat)
  │  ...                                  │
```

---
---

### Config YAML

```yaml
server:
  host: "0.0.0.0"
  port: 12345
  max_connections: 10000
  default_zoom: 1

paths:
  base_dir: "./data"
  icon: "{base}/x{zoom}/icon/{id}.png"
  effect: "{base}/x{zoom}/effectdata/{id}"
  map: "{base}/x{zoom}/map/{id}"
  npc: "{base}/x{zoom}/npc/{id}"
  background: "{base}/x{zoom}/background/{id}"
  tileset: "{base}/x{zoom}/tileset/{id}"
  image_source: "{base}/x{zoom}/imagesource"
  image_source2: "{base}/x{zoom}/imagesource2"
```

Cấu trúc thư mục data:
```
data/
├── x1/
│   ├── icon/
│   │   ├── 1.png
│   │   ├── 2.png
│   │   └── ...
│   ├── effectdata/
│   ├── map/
│   ├── npc/
│   └── ...
├── x2/
│   ├── icon/
│   ├── effectdata/
│   └── ...
└── x4/
    └── ...
```

---

### Fallback mechanism

```
Client muốn requestIcon(id):
  1. if (FREIGHT_ENABLED && Session_Freight.isConnected())
       → Service_Freight.requestIcon(id)      // gửi qua Freight
  2. else
       → original code                        // gửi qua Session_ME2 hoặc Session_ME
```

- `FREIGHT_ENABLED` là flag trong `Main.cs`, có thể tắt từ config
- Nếu Freight server down → `Session_Freight.isConnected()` trả `false` → tự động fallback
- `Service_Freight.GetSession()` cũng có fallback chain: Freight → ME2 → ME

---

### Commands được chuyển sang Freight

| Command | Byte | Tên | Big Data? |
|---|---|---|---|
| REQUEST_ICON | -67 | Icon sprites | ✅ |
| GET_EFFDATA | -66 | Effect animation data | ✅ |
| REQUEST_MAPTEMPLATE | 10 | Map template | ❌ |
| REQUEST_NPCTEMPLATE | 11 | NPC template | ✅ |
| GET_IMAGE_SOURCE | -74 | Image source pack | ✅ |
| GET_IMAGE_SOURCE2 | -111 | Image source 2 | ❌ |
| BACKGROUND_TEMPLATE | -32 | Background data | ✅ |
| TILE_SET | -82 | Tile set data | ❌ |
| SMALLIMAGE_VERSION | -77 | Version check | ❌ |
| BGITEM_VERSION | -93 | Version check | ❌ |
| **FREIGHT_INIT** | **1** | **zoomLevel handshake** | ❌ |

---

### Ước tính tối ưu băng thông

### Phân tích traffic game server hiện tại

| Loại | % bandwidth ước tính | Kích thước trung bình |
|---|---|---|
| Icon data (REQUEST_ICON) | ~30-40% | 2-50 KB/icon |
| Effect data (GET_EFFDATA) | ~15-20% | 5-100 KB/effect |
| Map template | ~10-15% | 10-200 KB/map |
| NPC template | ~5-10% | 1-20 KB/npc |
| Image source | ~5-10% | 50-500 KB (one-time) |
| Background + Tileset | ~5% | 5-50 KB |
| **Tổng data traffic** | **~70-85%** | |
| Game logic (chat, move, battle) | ~15-30% | < 1 KB/packet |

### Sau khi tách Freight

| Metric | Trước | Sau |
|---|---|---|
| Game server bandwidth | 100% | ~15-30% |
| Giảm tải | — | **~70-85%** |
| CCU capacity (cùng hardware) | N | **~3-5x N** |
| Freight server (Rust) | — | Handles data traffic |

### Tại sao Rust?
- **Tokio async I/O**: Không cần thread-per-connection, xử lý 10K+ connections trên 1 core
- **DashMap cache**: Lock-free concurrent cache, tránh đọc disk lặp lại
- **Zero-cost abstractions**: Không có GC pause như Java
- **Memory**: ~2-5 MB RSS cho server idle, mỗi connection thêm ~50 KB

---

### Cách chạy

### Freight Server
```bash
cd server
# Chuẩn bị data
mkdir -p data/x1/icon data/x2/icon data/x4/icon
# ... copy asset files ...

# Build & run
cargo build --release
./target/release/freight
# Hoặc dev mode:
cargo run
```

### Client
Trong Unity, đặt config:
```csharp
Main.FREIGHT_HOST = "192.168.1.100";  // IP của Freight server
Main.FREIGHT_PORT = 14445;
Main.FREIGHT_ENABLED = true;
```

---

### TODO / Cần làm thêm

- [✅] Populate thư mục `data/x{zoom}/` với asset files từ game server Java
- [✅] Xử lý reconnect tự động khi Freight mất kết nối
- [❌] Tab2 `Service2.cs` — thêm Freight routing (tương tự Service.cs)
- [✅] Monitoring / metrics (connections count, cache hit rate, bandwidth)
- [✅] TLS encryption (nếu Freight server trên public network)
- [✅] Rate limiting per IP
- [...] Hot-reload config khi file thay đổi
