// Artillery processor for router-metrics-exporter load tests
// Handles dynamic header generation and request customization

module.exports = {
  // Generate random nonce for replay protection
  generateNonce: generateNonce,
  
  // Add API key to request if configured
  addApiKey: addApiKey,
};

function generateNonce(requestParams, context, ee, next) {
  const nonce = `nonce-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  requestParams.headers = requestParams.headers || {};
  requestParams.headers['X-Nonce'] = nonce;
  return next();
}

function addApiKey(requestParams, context, ee, next) {
  if (context.vars.api_key) {
    requestParams.headers = requestParams.headers || {};
    requestParams.headers['X-API-Key'] = context.vars.api_key;
  }
  return next();
}
