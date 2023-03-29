(function() {var implementors = {
"subspace_transaction_pool":[["impl&lt;Block, Client, TxPreValidator&gt; ChainApi for <a class=\"struct\" href=\"subspace_transaction_pool/struct.FullChainApiWrapper.html\" title=\"struct subspace_transaction_pool::FullChainApiWrapper\">FullChainApiWrapper</a>&lt;Block, Client, TxPreValidator&gt;<span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;Block: BlockT,<br>&nbsp;&nbsp;&nbsp;&nbsp;Client: ProvideRuntimeApi&lt;Block&gt; + BlockBackend&lt;Block&gt; + BlockIdTo&lt;Block&gt; + HeaderBackend&lt;Block&gt; + HeaderMetadata&lt;Block, Error = Error&gt; + <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/marker/trait.Send.html\" title=\"trait core::marker::Send\">Send</a> + <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/marker/trait.Sync.html\" title=\"trait core::marker::Sync\">Sync</a> + 'static,<br>&nbsp;&nbsp;&nbsp;&nbsp;Client::Api: TaggedTransactionQueue&lt;Block&gt;,<br>&nbsp;&nbsp;&nbsp;&nbsp;TxPreValidator: <a class=\"trait\" href=\"subspace_transaction_pool/trait.PreValidateTransaction.html\" title=\"trait subspace_transaction_pool::PreValidateTransaction\">PreValidateTransaction</a>&lt;Block = Block&gt; + <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/marker/trait.Send.html\" title=\"trait core::marker::Send\">Send</a> + <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/marker/trait.Sync.html\" title=\"trait core::marker::Sync\">Sync</a> + <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html\" title=\"trait core::clone::Clone\">Clone</a> + 'static,</span>"]],
"substrate_test_runtime_transaction_pool":[["impl ChainApi for <a class=\"struct\" href=\"substrate_test_runtime_transaction_pool/struct.TestApi.html\" title=\"struct substrate_test_runtime_transaction_pool::TestApi\">TestApi</a>"]]
};if (window.register_implementors) {window.register_implementors(implementors);} else {window.pending_implementors = implementors;}})()