# Payments Engine
Run with `cargo run transactions.csv` or build with `cargo build --release` and then run the executable 
with the same CSV argument. Log level can be set with `RUST_LOG` environment variable.

# Basics
The application should build and run and read/write data as specified.
# Completeness
All cases should be handled properly, including disputes, resolutions and chargebacks.
# Correctness
The application is tested with unit test for each of the actions. Testing should further be improved 
with an integration test and more unit test coverage. Sample data is included in file `test.csv`.
# Safety and Robustness
Rust unsafe features are not used. Errors are being handled by custom `PaymentEngineResult` and `PaymentEngineError` types.
The errors are properly handled and logged with `env_logger`. Error handling can be improved by better handling errors
related to saving data on disk and writing better conversions to `PaymentEngineError`.
# Efficiency
Transactions are streamed from CSV (whole CSV file is not loaded to memory) and after processing saved on disk. It would be faster to hold 
all transaction data in memory but that is dangerous as application could run out of memory if large enough CSV is imported. When transaction is 
under dispute it is loaded into LRU cache so that once a resolution comes it can be retrieved faster. Account data is stored in-memory as maximum number 
of unique accounts is not large enough to cause problems with memory. Further improvements can be done, including adding a thread pool which
will group transactions by `client_id` and process them in order. Another improvement that comes to mind would be to
implement a faster storage method for `DatastoreOperations` trait (currently `pickledb` crate is used only as a proof of concept).
# Maintainability
The code is seperated into different files with a specific responsibility in mind, functions are not large and should be
easy to understand and maintain. 