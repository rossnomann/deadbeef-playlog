#!/usr/bin/env python3
import hashlib
import hmac
import json

from argparse import ArgumentParser
from http.server import HTTPServer, BaseHTTPRequestHandler


SECRET = b'secret'


def verify_signature(expected_signature, data):
    actual_signature = hmac.new(SECRET, digestmod=hashlib.sha256)
    actual_signature.update(data)
    actual_signature = actual_signature.hexdigest()
    return actual_signature == expected_signature


class RequestHandler(BaseHTTPRequestHandler):
    def do_POST(self):
        content_length = int(self.headers['Content-Length'])
        request_data = self.rfile.read(content_length)

        signature = self.headers.get('X-HMAC-Signature')
        if verify_signature(signature, request_data):
            try:
                data = json.loads(request_data)
                print(data)
            except json.DecoderError as exc:
                print('Failed to decode request data: {}'.format(exc))
                status = 400
            else:
                # 200 status code considered ok, client will retry request otherwise
                status = 200
        else:
            status = 403
            print('Signature verification failed')
        self.send_response(status)
        self.send_header('Content-type', 'text/plain')
        self.end_headers()
        # response body is not required


def main():
    parser = ArgumentParser()
    parser.add_argument(
        '--bind', '-b',
        default='',
        metavar='ADDRESS',
        help='Specify alternate bind address [default: all interfaces]'
    )
    parser.add_argument(
        'port',
        action='store',
        default=8000,
        type=int,
        nargs='?',
        help='Specify alternate port [default: 8000]'
    )
    args = parser.parse_args()
    server = HTTPServer((args.bind, args.port), RequestHandler)
    server_address = server.socket.getsockname()
    serve_message = "Serving HTTP on {host} port {port} (http://{host}:{port}/) ..."
    print(serve_message.format(host=server_address[0], port=server_address[1]))
    server.serve_forever()


if __name__ == '__main__':
    try:
        main()
    except KeyboardInterrupt:
        pass
