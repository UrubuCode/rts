// Request descriptor — usado pra montar a request HTTP/1.1 raw.

export class Request {
  url: string;
  method: string;
  headers: string;  // raw "Name: value\r\n..."
  body: string;

  constructor(url: string, method: string, headers: string, body: string) {
    this.url = url;
    this.method = method;
    this.headers = headers;
    this.body = body;
  }
}

export class RequestInit {
  method: string;
  headers: string;
  body: string;
  constructor(method: string, headers: string, body: string) {
    this.method = method;
    this.headers = headers;
    this.body = body;
  }
}

export function defaultInit(): RequestInit {
  return new RequestInit("GET", "", "");
}
