# Rust RDBMS

Rust로 직접 구현하는 관계형 데이터베이스 관리 시스템(RDBMS).

단순한 SQL Parser 구현이 아니라, 파일 기반 Storage Engine부터 Query Engine, Index, Transaction, WAL, MVCC, Client/Server 구조까지 직접 구현하는 것을 목표로 한다.

## Goals

최종적으로 다음과 같은 형태의 독립적인 RDBMS를 구현한다.

```text
Client
  │
  │ TCP
  ▼
RDBMS Server
  │
  ├── SQL Parser
  ├── Binder
  ├── Query Planner / Optimizer
  ├── Executor
  ├── Transaction Manager
  ├── Buffer Pool
  ├── Index
  └── Storage Engine
        │
        ▼
      Disk
```

최종 사용 형태:

```bash
mydb-server --data ./data

mydb-cli -h localhost -p 5433
```

```sql
CREATE TABLE users (
    id BIGINT,
    name VARCHAR
);

INSERT INTO users VALUES (1, 'Kim');

SELECT *
FROM users
WHERE id = 1;
```

---

# Milestones

## M1. Disk / Page Manager

DB 파일을 고정 크기 Page 단위로 관리한다.

기본 Page 크기:

```text
8 KB
```

### 구현

* [x] Database File 생성 / Open
* [x] `PageId`
* [x] 8KB Page
* [x] Page Allocate
* [x] Page Read
* [x] Page Write
* [x] Page Offset 계산
* [x] 프로그램 재시작 후 데이터 유지

### 구조

```text
table.data

┌──────────────┐
│ Page 0       │
│ 8192 bytes   │
├──────────────┤
│ Page 1       │
│ 8192 bytes   │
├──────────────┤
│ Page 2       │
│ 8192 bytes   │
└──────────────┘
```

```text
offset = page_id × PAGE_SIZE
```

### 완료 조건

특정 Page에 데이터를 기록하고 프로그램을 종료한 뒤 다시 실행해 동일한 데이터를 읽을 수 있다.

---

## M2. Slotted Page / Row

하나의 Page 내부에 여러 Row를 저장한다.

### 구현

* [x] Page Header
* [x] Slot Directory
* [x] Free Space 관리
* [x] Row 직렬화
* [x] Row 역직렬화
* [x] `RowId(PageId, SlotId)`
* [x] Row Insert
* [x] Row Read
* [x] Row Update
* [x] Row Delete
* [x] free block
* [x] lazy compress

### 구조

```text
Page

┌───────────────────────────┐
│ Header                    │
├───────────────────────────┤
│ Slot 0                    │
│ Slot 1                    │
│ Slot 2                    │
│                           │
│        Free Space         │
│                           │
│                    Row 2  │
│                    Row 1  │
│                    Row 0  │
└───────────────────────────┘
```

초기 버전에서는 하나의 Row가 Page 크기를 초과하면 `RowTooLarge` 오류로 처리한다.

---

## M3. Heap Table

여러 Page를 하나의 Table로 관리한다.

### 구현

* [x] Table File 생성
* [x] Page 추가
* [x] Row Insert
* [x] Row Get
* [x] Row Update
* [x] Row Delete
* [x] Full Table Scan
* [x] Row를 저장할 Page 선택
* [x] Page Free Space 관리

### 구조

```text
users.tbl

├── Page 0
│   ├── Row
│   └── Row
│
├── Page 1
│   ├── Row
│   ├── Row
│   └── Row
│
└── Page 2
```

### 완료 조건

SQL 없이 Storage API만 사용하여 CRUD가 가능하다.

```text
insert(row)
get(row_id)
update(row_id, row)
delete(row_id)
scan()
```

**Storage Engine v1 완료**

---

## M4. Schema / Catalog

관계형 데이터베이스의 Schema 정보를 관리한다.

### 구현

* [x] Database Metadata
* [x] Table Metadata
* [x] Column Metadata
* [x] Data Type
* [x] Catalog 저장
* [x] Catalog 재로딩

초기 지원 타입:

```text
INT
BIGINT
BOOLEAN
VARCHAR
NULL
```

예:

```sql
CREATE TABLE users (
    id BIGINT,
    name VARCHAR
);
```

Catalog 내부에서는 다음과 같은 정보를 관리한다.

```text
Table: users

Column
├── id   : BIGINT
└── name : VARCHAR
```

---

## M5. SQL Parser

SQL 문자열을 AST(Abstract Syntax Tree)로 변환한다.

### 구현

* [x] Lexer
* [x] Parser
* [x] AST
* [x] Literal
* [x] Expression

초기 지원 SQL:

* [x] `CREATE TABLE`
* [x] `INSERT`
* [x] `SELECT`
* [ ] `UPDATE`
* [ ] `DELETE`

### 흐름

```text
SQL
 ↓
Lexer
 ↓
Token
 ↓
Parser
 ↓
AST
```

예:

```sql
SELECT name
FROM users
WHERE id = 10;
```

```text
Select
├── Projection
│   └── name
├── Table
│   └── users
└── Filter
    └── Equal
        ├── id
        └── 10
```

---

## M6. Binder / Executor

SQL AST를 실제 Storage Engine의 작업으로 연결한다.

### Binder

* [ ] Table 존재 여부 확인
* [ ] Column 존재 여부 확인
* [ ] Column Type 확인
* [ ] Expression Type 검사
* [ ] 이름을 내부 ID로 변환

### Executor

* [ ] SeqScan
* [ ] Filter
* [ ] Projection
* [ ] Insert Executor
* [ ] Update Executor
* [ ] Delete Executor

### 흐름

```text
SQL
 ↓
Parser
 ↓
AST
 ↓
Binder
 ↓
Executor
 ↓
Heap Table
 ↓
Page
 ↓
File
```

### 완료 조건

다음 SQL CRUD가 실제 파일을 대상으로 동작한다.

```sql
CREATE TABLE users (
    id BIGINT,
    name VARCHAR
);

INSERT INTO users VALUES (1, 'Kim');

SELECT * FROM users;

SELECT name
FROM users
WHERE id = 1;

UPDATE users
SET name = 'Lee'
WHERE id = 1;

DELETE FROM users
WHERE id = 1;
```

**RDBMS v0.1 완료**

---

## M7. Buffer Pool

Disk Page를 메모리에 캐싱한다.

매 Query마다 Disk I/O를 수행하지 않고 Buffer Pool을 통해 Page를 관리한다.

### 구현

* [ ] Buffer Frame
* [ ] Page Table
* [ ] Page Fetch
* [ ] Pin / Unpin
* [ ] Dirty Page
* [ ] Flush
* [ ] Page Eviction
* [ ] Clock Replacement

### 구조

```text
Executor
   │
   ▼
Buffer Pool
├── Page 10
├── Page 42
└── Page 81
   │
   ▼
Disk
```

---

## M8. B+Tree Index

Full Table Scan 없이 데이터를 검색할 수 있도록 B+Tree Index를 구현한다.

### 구현

* [ ] B+Tree Page Format
* [ ] Leaf Node
* [ ] Internal Node
* [ ] Search
* [ ] Insert
* [ ] Leaf Split
* [ ] Internal Split
* [ ] Root Split
* [ ] Delete
* [ ] Merge
* [ ] `CREATE INDEX`
* [ ] Index Scan

예:

```sql
CREATE INDEX idx_users_id
ON users(id);
```

```text
id = 100
   ↓
B+Tree
   ↓
RowId(PageId, SlotId)
   ↓
Heap Table
```

---

## M9. Query Engine 확장

일반적인 관계형 Query 기능을 추가한다.

### 구현

* [ ] `AND`
* [ ] `OR`
* [ ] Comparison Expression
* [ ] `ORDER BY`
* [ ] `LIMIT`
* [ ] Aggregate
* [ ] `COUNT`
* [ ] `SUM`
* [ ] `GROUP BY`
* [ ] Nested Loop Join
* [ ] Hash Join

예:

```sql
SELECT department_id, COUNT(*)
FROM employees
GROUP BY department_id;
```

```sql
SELECT u.name, o.amount
FROM users u
JOIN orders o
    ON u.id = o.user_id;
```

---

## M10. Query Planner / Optimizer

하나의 SQL을 어떤 방법으로 실행할지 결정한다.

### 구현

* [ ] Logical Plan
* [ ] Physical Plan
* [ ] SeqScan
* [ ] IndexScan
* [ ] NestedLoopJoin
* [ ] HashJoin
* [ ] Table Statistics
* [ ] 간단한 Cost Model
* [ ] Plan 선택

예:

```sql
SELECT *
FROM users
WHERE id = 100;
```

가능한 Plan:

```text
SeqScan(users)
```

또는:

```text
IndexScan(users_id_idx)
```

Optimizer가 더 적절한 Plan을 선택한다.

**Query Engine v1 완료**

---

## M11. Transaction / Lock

ACID Transaction의 기본 기능을 구현한다.

### 구현

* [ ] Transaction ID
* [ ] Transaction State
* [ ] `BEGIN`
* [ ] `COMMIT`
* [ ] `ROLLBACK`
* [ ] Shared Lock
* [ ] Exclusive Lock
* [ ] Row Lock
* [ ] Two-Phase Locking
* [ ] Deadlock 처리

예:

```sql
BEGIN;

UPDATE accounts
SET balance = balance - 100
WHERE id = 1;

UPDATE accounts
SET balance = balance + 100
WHERE id = 2;

COMMIT;
```

---

## M12. WAL / Crash Recovery

프로세스가 비정상 종료되어도 데이터베이스를 복구할 수 있도록 한다.

### 구현

* [ ] WAL File
* [ ] WAL Record
* [ ] LSN
* [ ] WAL Flush
* [ ] Redo
* [ ] Undo
* [ ] Recovery
* [ ] Checkpoint

기본 원칙:

```text
Page를 Disk에 쓰기 전

WAL 기록
   ↓
WAL fsync
   ↓
Data Page Flush
```

### 테스트

```text
Transaction 실행
      ↓
데이터 변경
      ↓
kill -9
      ↓
DB 재시작
      ↓
WAL Recovery
      ↓
Consistency 검증
```

---

## M13. MVCC

여러 Transaction이 동시에 데이터를 처리할 수 있도록 MVCC를 구현한다.

### 구현

* [ ] Tuple Version
* [ ] Transaction Snapshot
* [ ] `xmin`
* [ ] `xmax`
* [ ] Tuple Visibility
* [ ] READ COMMITTED
* [ ] REPEATABLE READ
* [ ] Dead Tuple 관리
* [ ] Vacuum

개념적인 Tuple Header:

```text
Tuple
├── xmin
├── xmax
└── Data
```

**Transaction Engine v1 완료**

---

## M14. Client / Server

RDBMS를 독립적인 Server Process로 실행할 수 있도록 한다.

### 구현

* [ ] TCP Listener
* [ ] Connection
* [ ] Session
* [ ] Concurrent Client
* [ ] Current Database
* [ ] Session Transaction
* [ ] Wire Protocol
* [ ] CLI Client
* [ ] Authentication

### 구조

```text
mydb-cli
    │
    │ TCP
    ▼
mydb-server
    │
    ├── Session
    ├── SQL Engine
    ├── Transaction
    └── Storage Engine
```

실행:

```bash
mydb-server --data ./data
```

```bash
mydb-cli -h localhost -p 5433
```

---

# Roadmap

```text
M1  Disk / Page Manager
 ↓
M2  Slotted Page / Row
 ↓
M3  Heap Table
 ↓
M4  Schema / Catalog
 ↓
M5  SQL Parser
 ↓
M6  Binder / Executor
 │
 └──── RDBMS v0.1
 ↓
M7  Buffer Pool
 ↓
M8  B+Tree Index
 ↓
M9  Query Engine
 ↓
M10 Query Planner / Optimizer
 │
 └──── Query Engine v1
 ↓
M11 Transaction / Lock
 ↓
M12 WAL / Recovery
 ↓
M13 MVCC
 │
 └──── Transaction Engine v1
 ↓
M14 Client / Server
 │
 └──── RDBMS v1
```

---

# Initial Target

당장의 목표는 **M1 ~ M6**이다.

```text
Disk / Page
    ↓
Slotted Page
    ↓
Heap Table
    ↓
Catalog
    ↓
SQL Parser
    ↓
Executor
```

첫 번째 완성 버전에서는 다음 조건을 만족한다.

* 실제 파일에 데이터 저장
* 프로그램 재시작 후 데이터 유지
* Table 생성 가능
* Row Insert 가능
* Row 조회 가능
* Row 수정 가능
* Row 삭제 가능
* 간단한 `WHERE` 조건 처리 가능

이후 Index, Buffer Pool, Query Optimizer, Transaction, WAL, MVCC, Client/Server 순으로 확장한다.
