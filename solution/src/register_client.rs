// Register client odpowiada za komunikacje, 
// ale mozna myslec o nim jako o zbiorze stubborn linkow 
// do kazdego innego procesu i mi taka abstrakcja siada w glowie

// RegisterClient manages TCP communication between processes of the distributed register. 
// An instance is passed to instances of AtomicRegister to allow them communicating with each other.

// 1. Messages sent by a process to itself should skip TCP, serialization, deserialization, 
// HMAC preparation and validation phases to improve the performance
