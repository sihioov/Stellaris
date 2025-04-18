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