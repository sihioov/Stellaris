# ✂️ Stellaris Snippet Collection

> It is a document that collects frequently used code fragments, setup examples, tricks, and more.

## 2025-04-18
rust trait(행동) + struct(data) = c++ class
	이렇게 한 이유는 조합에 다양성
	정적 디스패치의 최적화
	명확한 소유권 - 데이터 담은 struct가 어디서 어떻게 이동,빌림 되는지 코드에 드러남

Rust - async

trait = 추상 인터페이스라고 생각?
dyn trait = 트레잇 객체
dyn trait를 만들려면 힙을 할당해야 함(Box)
	그러면 생성자는 없는건가??


into_iter = 변수의 소유권을 가지고 옴
iter	= 참조로 가져옴(읽기 전용)
iter_mut = 가변 참조로 순회 (값 수정가능)

- crate는 최소의 컴파일 단위이다, 하나의 독립된 컴파일 단위
- crate:: (루트에서 시작)
- binary crate, library crate 존재

Rust if는 뭔가 좀 다르다 if 자체가 값이 되는 표현식

Rust에서는 아주 간단한 값(i32, bool, char, f64 등) 만 Copy trait가 구현되어 있음
그 외 대부분의 구조체(Vec, String, HashMap, TaskMessage, ...)는 **move가 기본.**

- clone() => 값 복사, cloned() => 참조 복사


- **표현식(expression)**: **“값을 만들어내는 코드 조각”**
- **“문장(statement)”**: **"값을 만들어내지 않고 그냥 어떤 동작만 수행하는 코드"**

## 2025-04-25

### self, &self, mut self, &mut self

### self
- Ownership: 메서드 호출 시 인스턴트를 소비(move)
- Concurrency: 한 번만 호출 가능, 동시에 사용할 수 없음
- Trait 객체: dyn Trait에선 사용할 수 없음
- 주 용도: 이 객체를 더 이상 쓰지 않을 때 한 번만 수행하는 작업

### mut self
- Ownership: self와 동일하게 소비(move)
- 차이점: 함수 몸체 안에서 로컬 변수처럼 mutable 바인딩
- Concurrency & Trait 객체: self와 동일 제약
- 주 용도: 이동하면서 내부 값을 한 번만 수정할 때

### &mut self
- Ownership: 인스턴트를 소비하지 않고 가변 참조(mut borrow)
- Concurrency: 오직 하나의 가변 참조만 허용 → 동시 호출 불가
- Trait 객체: 제한적으로 사용 가능하나, 빌림 규칙 때문에 복잡
- 주 용도: 메서드 안에서 구조체 필드를 직접 바꿔야 할 때

### &self
- Ownership: 인스턴트 유지, 불변 참조(immutable borrow)
- Concurrency: 여러 비동기 태스크에서 안전하게 동시에 호출 가능
- Trait 객체: dyn Trait 형태로도 자유롭게 사용 가능
- 주 용도: 내부에 Mutex/RwLock 같은 잠금장치를 써서 상태 변경이 필요하거나, 단순 읽기용으로 여러 곳에서 재사용할 때

### Send
- 인스턴스를 스레드 사이로 옮겨도 안전

### Sync
- 타입에 대한 참조를 스레드 사이로 공유해도 안전

### 'static
- 스레드를 띄워 놓고도 참조가 사라지지 않도록 프로그램 전체 수명 동안 보장
- MQ 클라이언트를 Tokio 태스크나 스레드 풀에서 마음껏 옮겨 쓰려면, 이 세 가지 바운드를 주로 붙여 줍니다.

### tokio::spawn
- 경량 스레드