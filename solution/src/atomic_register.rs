// Atomic register implementation as presented in the task description.

// All its methods shall follow the atomic register algorithm presented above. 
// When implementing AtomicRegister, you can assume that 
// RegisterClient passed to the function implements StubbornLink required by the algorithm.

// Every sector is logically a separate atomic register. 
// However, you should not keep Configuration.public.n_sectors AtomicRegister objects in memory; 
// instead, you should dynamically create and delete them to limit the memory usage (see also the Technical Requirements section).
