# Technical Implementation Details

This document provides an in-depth explanation of how LocalSend Rust works internally, covering the core technologies, protocols, and design decisions.

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Device Discovery](#device-discovery)
3. [Protocol Communication](#protocol-communication)
4. [Server Implementation](#server-implementation)
5. [Client Implementation](#client-implementation)
6. [File Transfer Mechanics](#file-transfer-mechanics)
7. [Security Model](#security-model)
8. [State Machine](#state-machine)
9. [Type Safety](#type-safety)

---

## Architecture Overview

### System Architecture Diagram

```mermaid
flowchart TB
    subgraph LocalNetwork["Local Network"]
        subgraph DeviceA["Device A (Sender)"]
            CLI_A["CLI/TUI"]
            Client_A["HTTP Client"]
            Discovery_A["UDP Multicast"]
        end
        
        subgraph DeviceB["Device B (Receiver)"]
            Server_B["Axum HTTP Server"]
            TUI_B["Terminal UI"]
            Discovery_B["UDP Multicast"]
        end
        
        Network["Local Network<br/>224.0.0.167:53317"]
    end
    
    CLI_A -->|"1. Announce"| Network
    Network -->|"2. Discover"| Server_B
    Server_B -->|"3. Register"| Network
    Network -->|"4. Device List"| CLI_A
    Client_A -->|"5. Upload Files"| Server_B
    
    style LocalNetwork fill:#e1f5fe
    style DeviceA fill:#fff3e0
    style DeviceB fill:#e8f5e9
```

### Module Structure

```mermaid
graph TD
    User["User"] --> CLI["CLI Commands"]
    User --> TUI["Terminal UI"]
    
    subgraph ApplicationLayer["Application Layer"]
        CLI --> Discovery["Discovery Module"]
        CLI --> Client["Client Module"]
        TUI --> Discovery
        TUI --> Client
    end
    
    subgraph ProtocolLayer["Protocol Layer"]
        Discovery --> Protocol["Protocol Types"]
        Client --> Protocol
        Server --> Protocol
        Protocol --> Validation["Validation"]
    end
    
    subgraph InfrastructureLayer["Infrastructure Layer"]
        Discovery --> Network["UDP Multicast"]
        Client --> HTTP["Reqwest HTTP"]
        Server --> Axum["Axum Framework"]
        Server --> TLS["TLS/Rustls"]
    end
    
    subgraph StorageLayer["Storage Layer"]
        Server --> FileSystem["FileSystem Trait"]
        FileSystem --> TokioFS["Tokio FileSystem"]
        FileSystem --> MemoryFS["Memory FS<br/>(for testing)"]
    end
    
    style ProtocolLayer fill:#e3f2fd
    style InfrastructureLayer fill:#fce4ec
    style StorageLayer fill:#f3e5f5
```

---

## Device Discovery

### Multicast UDP Discovery

LocalSend uses **UDP multicast** to discover devices on the local network. This allows multiple devices to find each other without a central server.

#### Multicast Configuration

| Parameter | Value | Description |
|-----------|-------|-------------|
| **Multicast IP** | `224.0.0.167` | IPv4 multicast address (organization-local) |
| **Port** | `53317` | Default LocalSend port |
| **Broadcast Interval** | 1 second | How often devices announce themselves |
| **Discovery Timeout** | 10 seconds | How long to wait for responses |

#### Discovery Protocol

```mermaid
sequenceDiagram
    participant S as Sender
    participant N as Network\n224.0.0.167:53317
    participant R as Receiver
    
    S->>N: Announce Presence\n{version: "2.1", alias: "My Device", ...}
    %% Multicast to all devices
    
    R->>N: Listen for announcements
    N-->>R: Receive announcement
    R-->>S: HTTP GET /info\n{fingerprint, httpsPort, ...}
    
    %% Device discovered! Sender now knows about Receiver
```

#### Implementation Details

**Announcement Message (UDP):**
```rust
// Sent via UDP to 224.0.0.167:53317
#[derive(Serialize)]
struct Announcement {
    version: String,           // "2.1"
    alias: String,            // Device name
    device_type: DeviceType,  // Mobile/Desktop
    device_model: String,     // "iPhone 15", "MacBook Pro", etc.
    fingerprint: String,      // SHA-256 hash for TLS
    port: u16,                // HTTPS port (usually 53317)
    protocol: Protocol,       // HTTPS/HTTP
    download: bool,           // Supports download API
}
```

**Discovery Process:**
```rust
impl MulticastDiscovery {
    pub async fn announce_presence(&self) {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        let addr = "224.0.0.167:53317".parse().unwrap();
        
        // Enable multicast
        socket.set_multicast_loop_v4(true)?;
        socket.set_multicast_ttl(4)?;
        
        let message = Announcement { /* ... */ };
        let data = serde_json::to_vec(&message)?;
        
        // Broadcast to network
        socket.send_to(&data, addr).await?;
    }
    
    pub async fn discover(&self, timeout: Duration) -> Vec<DeviceInfo> {
        let socket = UdpSocket::bind("0.0.0.0:53317").await?;
        
        let mut devices = Vec::new();
        let start = Instant::now();
        
        while start.elapsed() < timeout {
            // Try to receive announcements
            if let Ok((data, _)) = socket.recv_from(&mut buffer).await {
                let announcement: Announcement = serde_json::from_slice(&data)?;
                
                // Fetch device info via HTTP
                let device_info = self.fetch_device_info(&announcement).await?;
                devices.push(device_info);
            }
        }
        
        devices
    }
}
```

### HTTP Fallback Discovery

If UDP multicast is blocked (e.g., corporate networks), LocalSend falls back to HTTP-based discovery:

```mermaid
flowchart LR
    A[Sender] -->|HTTP GET| B[Broadcast IP:53317/info]
    B -->|JSON Response| A[DeviceInfo]
```

---

## Protocol Communication

### API Endpoints

LocalSend v2 uses a REST API over HTTP/HTTPS:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/localsend/v2/info` | GET | Get device information |
| `/api/localsend/v2/register` | POST | Register for session |
| `/api/localsend/v2/prepare-upload` | POST | Prepare file upload |
| `/api/localsend/v2/upload` | POST | Upload file content |
| `/api/localsend/v2/cancel` | POST | Cancel transfer |

### Protocol Flow

```mermaid
sequenceDiagram
    participant S as Sender
    participant R as Receiver
    
    S->>R: POST /api/localsend/v2/register\n{sessionId, files, ...}
    R-->>S: 200 OK
    
    %% Session established!
    
    S->>R: POST /api/localsend/v2/prepare-upload\n{files: [{fileId, fileName, ...}]}
    R-->>S: 200 OK\n{tokens: {fileId: token}}
    
    loop For each file
        S->>R: POST /api/localsend/v2/upload\n?sessionId=xxx&fileId=xxx&token=xxx\n[Binary file data]
        R-->>S: 200 OK
    end
    
    S->>R: POST /api/localsend/v2/cancel\n{sessionId: xxx}
    R-->>S: 200 OK
```

### Type-Safe Identifiers

To prevent errors, LocalSend Rust uses newtype pattern for protocol identifiers:

```rust
// Strong types prevent mixing up IDs at compile time
pub struct SessionId(String);      // Unique per transfer session
pub struct FileId(String);         // Unique per file in session
pub struct Token(String);          // Per-file authorization
pub struct Port(u16);              // Valid port numbers
pub enum Protocol { Http, Https }  // Protocol selection

// Usage prevents mistakes:
fn upload_file(
    session_id: &SessionId,      // Can't accidentally pass FileId here!
    file_id: &FileId,
    token: &Token,
    port: Port,
    protocol: Protocol,
) -> Result<()> { /* ... */ }
```

---

## Server Implementation

### Axum Framework

The server uses [Axum](https://github.com/tokio-rs/axum), a type-safe web framework for Rust:

```mermaid
graph LR
    Request["HTTP Request"] --> Router["Axum Router"]
    Router --> Handler1["/info"]
    Router --> Handler2["/register"]
    Router --> Handler3["/prepare-upload"]
    Router --> Handler4["/upload"]
    Router --> Handler5["/cancel"]
    
    Handler1 --> State["Shared State<br/>Arc<RwLock<ServerState>>"]
    Handler2 --> State
    Handler3 --> State
    Handler4 --> State
    Handler5 --> State
    
    State --> Files["File System"]
    State --> Sessions["Active Sessions"]
    State --> ReceivedFiles["Received Files List"]
```

### Server State Management

```rust
#[derive(Clone)]
struct ServerState {
    device_info: DeviceInfo,
    save_dir: PathBuf,
    current_session: Option<ActiveSession>,
    pending_transfer: Arc<RwLock<Option<PendingTransfer>>>,
    received_files: Arc<RwLock<Vec<ReceivedFile>>>,
}

struct ActiveSession {
    session_id: SessionId,
    sender_alias: String,
    files: HashMap<FileId, FileMetadata>,
    tokens: HashMap<FileId, Token>,
    created_at: Instant,
    last_activity: Instant,
}
```

### Handler Example

```rust
#[axum::debug_handler]
async fn handle_register(
    State(state_ref): State<Arc<RwLock<ServerState>>>,
    Json(body): Json<RegisterRequest>,
) -> impl IntoResponse {
    let state = state_ref.write().await;
    
    // Create session
    let session = ActiveSession {
        session_id: body.session_id,
        sender_alias: body.alias,
        files: body.files.into_iter()
            .map(|f| (f.id.clone(), f))
            .collect(),
        tokens: HashMap::new(),
        created_at: Instant::now(),
        last_activity: Instant::now(),
    };
    
    state.current_session = Some(session);
    StatusCode::OK
}
```

### Async I/O for File Operations

File operations use `tokio::fs` to avoid blocking the async runtime:

```rust
async fn save_file(save_path: &Path, data: Bytes) -> Result<()> {
    // Async file write - doesn't block other tasks
    tokio::fs::write(save_path, data).await?;
    Ok(())
}

async fn create_directories(path: &Path) -> Result<()> {
    // Async directory creation
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    Ok(())
}
```

---

## Client Implementation

### HTTP Client with Streaming

The client uses [Reqwest](https://docs.rs/reqwest/) for HTTP operations:

```mermaid
flowchart TB
    subgraph Client["HTTP Client"]
        ClientHttp["Reqwest Client"]
        Streamer["ReaderStream"]
        Body["HTTP Body<br/>(wrap_stream)"]
    end
    
    subgraph File["File Operations"]
        FileOpen["tokio::fs::File"]
        Metadata["Get file size"]
        Chunk["8KB chunks"]
    end
    
    subgraph Network["Network"]
        Request["HTTP POST"]
        Response["200 OK"]
    end
    
    FileOpen --> Metadata
    Metadata --> Streamer
    Streamer --> Chunk
    Chunk --> Body
    Body --> Request
    Request --> Response
```

### Streaming File Upload

For memory efficiency, large files are streamed instead of loaded entirely:

```rust
pub async fn upload_file(
    &self,
    target: &DeviceInfo,
    session_id: &SessionId,
    file_id: &FileId,
    token: &Token,
    file_path: &Path,
    progress: Option<ProgressCallback>,
) -> Result<()> {
    let url = format!(
        "{}://{}:{}/api/localsend/v2/upload?sessionId={}&fileId={}&token={}",
        target.protocol, target.ip, target.port,
        session_id, file_id, token
    );
    
    // Open file asynchronously
    let file = File::open(file_path).await?;
    let total_bytes = file.metadata().await?.len();
    
    // Create streaming body
    let stream = ReaderStream::new(file);
    let body = Body::wrap_stream(stream);
    
    // Send with streaming (not entire file in memory!)
    let response = self.client.post(&url).body(body).send().await?;
    
    match response.status() {
        StatusCode::OK | StatusCode::NO_CONTENT => Ok(()),
        _ => Err(LocalSendError::http_failed(response.status().as_u16(), "Upload failed"))
    }
}
```

**Memory Comparison:**

| Approach | Memory Usage | Large File Support |
|----------|-------------|-------------------|
| Load entire file | O(file_size) | Limited by RAM |
| Streaming | O(buffer_size) | Unlimited |

---

## File Transfer Mechanics

### Transfer States

```mermaid
stateDiagram-v2
    [*] --> Idle: Start application
    
    Idle --> WaitingForAcceptance: User initiates transfer
    WaitingForAcceptance --> Transferring: Receiver accepts
    WaitingForAcceptance --> Cancelled: Receiver rejects/times out
    
    Transferring --> Completed: All files received
    Transferring --> Cancelled: User cancels
    
    Completed --> Idle: Transfer complete
    Cancelled --> Idle: Transfer failed/cancelled
    
    note right of Completed: Transfer successful!<br/>Files saved to disk
    note right of Cancelled: Transfer aborted<br/>Cleanup performed
```

### Transfer State Machine Implementation

```rust
pub enum TransferState {
    Idle,
    WaitingForAcceptance {
        sender: DeviceInfo,
        files: Vec<FileMetadata>,
        timeout: Duration,
    },
    Transferring {
        session_id: SessionId,
        sender: DeviceInfo,
        files: Vec<FileMetadata>,
        completed: usize,  // Number of completed files
    },
    Completed {
        session_id: SessionId,
        total_files: usize,
    },
    Cancelled {
        reason: String,
    },
}

impl TransferState {
    /// Type-safe state transitions
    pub fn accept(self, session_id: SessionId) -> Result<TransferState, TransferError> {
        match self {
            TransferState::WaitingForAcceptance { sender, files, .. } => {
                Ok(TransferState::Transferring {
                    session_id,
                    sender,
                    files,
                    completed: 0,
                })
            }
            _ => Err(TransferError::InvalidStateTransition),
        }
    }
}
```

### Session Management

```rust
pub struct Session {
    id: SessionId,
    files: HashMap<FileId, FileMetadata>,
    tokens: HashMap<FileId, Token>,
    created_at: Instant,
    last_activity: Instant,
}

impl Session {
    /// Generate token for file
    pub fn generate_token(&mut self, file_id: &FileId) -> Token {
        let token = Token::new(&self.id, file_id);
        self.tokens.insert(file_id.clone(), token.clone());
        token
    }
    
    /// Check if session is expired (5 minute timeout)
    pub fn is_expired(&self) -> bool {
        self.last_activity.elapsed() > Duration::from_secs(5 * 60)
    }
}
```

---

## Security Model

### TLS/HTTPS Security

```mermaid
flowchart LR
    subgraph TLS["TLS 1.3 / Rustls"]
        CertGen["Certificate Generation<br/>(rcgen)"]
        Fingerprint["SHA-256 Fingerprint"]
        Handshake["TLS Handshake"]
    end
    
    A["Sender"] <-->|"Encrypted"| TLS
    TLS <-->|"Encrypted"| B["Receiver"]
    
    %% Note: All data encrypted, Certificate fingerprint verified
```

### Certificate Generation

```rust
// Self-signed certificate with rcgen
pub fn generate_certificate() -> (Vec<u8>, Vec<u8>, String) {
    let mut cert_gen = CertificateGenerator::new();
    
    cert_gen
        .subject_name("LocalSend Device")
        .validity_period_days(365)
        .signing_key_gen(SigningKeyGen::Rsa { key_size: 2048 });
    
    let (cert_pem, key_pem) = cert_gen.generate_pem();
    let fingerprint = sha256_from_pem(&cert_pem);
    
    (cert_pem, key_pem, fingerprint)
}
```

### Device Fingerprint

```rust
// SHA-256 fingerprint of certificate
pub fn generate_fingerprint() -> String {
    let cert_pem = get_certificate_pem();
    let cert_der = pem::parse(cert_pem)
        .and_then(|p| X509::from_der(&p.contents).ok())
        .expect("Valid certificate");
    
    let public_key = cert_der.public_key().unwrap();
    let public_key_bytes = public_key.to_der().unwrap();
    
    sha256_from_bytes(&public_key_bytes)
}
```

### Security Features

| Feature | Implementation | Purpose |
|---------|---------------|---------|
| **TLS Encryption** | Rustls, TLS 1.3 | Data in transit encryption |
| **Certificate Fingerprint** | SHA-256 of public key | Device authentication |
| **Token Authorization** | Per-file tokens | Prevent unauthorized uploads |
| **Session Timeout** | 5-minute inactivity | Auto-cleanup stale sessions |

---

## Type Safety

### Newtype Pattern

LocalSend Rust uses newtype wrappers for compile-time safety:

```rust
// Newtypes prevent mixing up identifiers
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct SessionId(String);

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct FileId(String);

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Token(String);

#[derive(Clone, Debug)]
pub struct Port(u16);

// Validation in constructor
impl SessionId {
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        assert!(!id.is_empty(), "SessionId cannot be empty");
        assert!(id.len() <= 64, "SessionId too long");
        Self(id)
    }
}
```

### Structured Error Handling

```rust
#[non_exhaustive]
pub enum LocalSendError {
    Network {
        message: String,
    },
    SessionNotFound {
        session_id: SessionId,
    },
    FileNotFound {
        file_id: FileId,
        session_id: SessionId,
    },
    VersionMismatch {
        expected: String,
        actual: String,
    },
    InvalidState {
        current: TransferState,
        attempted: &'static str,
    },
}

impl std::fmt::Display for LocalSendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LocalSendError::Network { message } => 
                write!(f, "Network error: {}", message),
            LocalSendError::SessionNotFound { session_id } =>
                write!(f, "Session not found: {}", session_id),
            // ...
        }
    }
}
```

### Protocol Validation

```rust
pub mod validation {
    pub fn validate_version(version: &str) -> Result<()> {
        const SUPPORTED_VERSION: &str = "2.1";
        if version != SUPPORTED_VERSION {
            Err(LocalSendError::VersionMismatch {
                expected: SUPPORTED_VERSION.to_string(),
                actual: version.to_string(),
            })
        } else {
            Ok(())
        }
    }
    
    pub fn validate_device_info(info: &DeviceInfo) -> Result<()> {
        validate_version(&info.version)?;
        if info.alias.is_empty() {
            return Err(LocalSendError::InvalidDevice {
                reason: "Alias cannot be empty".to_string(),
            });
        }
        if info.port == 0 {
            return Err(LocalSendError::InvalidDevice {
                reason: "Port must be non-zero".to_string(),
            });
        }
        Ok(())
    }
}
```

---

## Performance Optimizations

### Async I/O

```mermaid
flowchart LR
    subgraph Before["Blocking I/O (Bad)"]
        A1["Request 1"] --> F1["fs::write"]
        A2["Request 2"] --> F2["fs::write"]
        A3["Request 3"] --> F3["fs::write"]
        F1 & F2 & F3 --> B["Thread Pool<br/>Starvation!"]
    end
    
    subgraph After["Async I/O (Good)"]
        B1["Request 1"] --> T1["tokio::fs::write"]
        B2["Request 2"] --> T2["tokio::fs::write"]
        B3["Request 3"] --> T3["tokio::fs::write"]
        T1 & T2 & T3 --> C["Event Loop<br/>Concurrent!"]
    end
```

### Memory-Efficient Streaming

```rust
// Instead of loading entire file:
let data = tokio::fs::read("large_file.iso").await?; // OOM for 10GB file!

// Use streaming:
let file = File::open("large_file.iso").await?;
let stream = ReaderStream::new(file); // Only 8KB buffer
let body = Body::wrap_stream(stream);
```

**Memory Comparison:**
- **Blocking approach**: ~10GB RAM for 10GB file
- **Streaming approach**: ~8KB RAM for 10GB file

### Lock Strategy

```mermaid
flowchart TB
    subgraph Server["Server (Async Context)"]
        L1["tokio::sync::RwLock"]
        A1[".read().await"]
        A2[".write().await"]
    end
    
    subgraph TUI["TUI (Sync Render)"]
        L2["tokio::sync::RwLock"]
        B1[".try_read()<br/>(non-blocking)"]
        B2[".try_write()<br/>(non-blocking)"]
    end
    
    L1 --> A1
    L1 --> A2
    L2 --> B1
    L2 --> B2
    
    %% Note: Use .await in async handlers
    %% Note: Use try_read/try_write in render
```

---

## Key Technologies Summary

| Layer | Technology | Purpose |
|-------|------------|---------|
| **Runtime** | Tokio | Async runtime for Rust |
| **Web Framework** | Axum | Type-safe HTTP server |
| **HTTP Client** | Reqwest | HTTP client with streaming |
| **Protocol** | LocalSend v2 | File transfer protocol |
| **Discovery** | UDP Multicast | Device discovery |
| **TLS** | Rustls | TLS 1.3 encryption |
| **UI** | Ratatui | Terminal UI framework |
| **CLI** | Clap | Command-line argument parsing |
| **Serialization** | Serde | JSON serialization/deserialization |
| **Validation** | Thiserror + Anyhow | Error handling |

---

## Data Flow Diagrams

### Send File Flow

```mermaid
sequenceDiagram
    participant U as User
    participant CLI as CLI/TUI
    participant Client as HTTP Client
    participant S as Server
    participant FS as File System
    
    U->>CLI: send <target> <file>
    CLI->>Client: prepare_upload(target, files)
    Client->>S: POST /prepare-upload
    S-->>Client: Response with tokens
    
    %% Session established!
    
    loop For each file
        CLI->>Client: upload_file(target, session, file)
        Client->>FS: Open file (async)
        FS-->>Client: File handle
        Client->>Client: Create ReaderStream
        Client->>S: POST /upload (streaming body)
        S->>FS: Write to disk (async)
        FS-->>S: Write complete
        S-->>Client: 200 OK
    end
    
    Client-->>CLI: Transfer complete
    CLI->>U: Display success
```

### Receive File Flow

```mermaid
sequenceDiagram
    participant S as Sender
    participant Server as Axum Server
    participant State as Server State
    participant FS as File System
    participant TUI as TUI
    
    S->>Server: POST /register\n(sessionId, files)
    Server->>State: Store session
    State-->>Server: Session stored
    Server-->>S: 200 OK
    
    loop For each file
        S->>Server: POST /upload\n?sessionId=...&fileId=...&token=...\n[Binary data]
        Server->>State: Validate token
        State-->>Server: Token valid
        Server->>FS: Create directories\n(async)
        FS-->>Server: Directories created
        Server->>FS: Write file (async)
        FS-->>Server: File saved
        Server->>State: Update progress
        State-->>TUI: Notify new file
        Server-->>S: 200 OK
    end
    
    S->>Server: POST /cancel
    Server->>State: Clear session
    State-->>Server: Session cleared
    Server-->>S: 200 OK
```

---

## Contributing

See [AGENTS.md](AGENTS.md) for development guidelines and [CHANGELOG.md](CHANGELOG.md) for version history.

---

## References

- [LocalSend Protocol Specification](https://github.com/localsend/protocol)
- [Rust Tokio Runtime](https://tokio.rs/)
- [Axum Web Framework](https://github.com/tokio-rs/axum)
- [Reqwest HTTP Client](https://docs.rs/reqwest/)
- [Rustls TLS](https://github.com/rustls/rustls)
- [Ratatui TUI](https://github.com/ratatui-org/ratatui)
