package provapdf

import "encoding/json"

// jsonMarshal is a package-internal alias to avoid importing encoding/json
// in every file that needs a one-off marshal call.
var jsonMarshal = json.Marshal
