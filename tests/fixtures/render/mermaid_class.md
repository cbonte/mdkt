```mermaid
classDiagram
  class Animal {
    +int age
    +eat()
  }
  Animal <|-- Dog
  Dog ..> Food : eats
```

```mermaid
classDiagram
  namespace Zoo {
    class Cage
  }
```
