// The extension's entry point: starts the bridge (bridge.js).
//
// It starts unconditionally. Claude Desktop's built-in Node runs an
// extension in a utility process whose main script is Claude's own host,
// which loads this file with import(); require.main is the host there, so
// a "require.main === module" check would never start the bridge.
'use strict';

require('./bridge.js').main();
