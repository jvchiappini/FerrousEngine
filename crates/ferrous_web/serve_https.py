import http.server
import ssl
import os

# Generar certificado auto-firmado si no existe
if not os.path.exists("server.pem"):
    print("Generando certificado auto-firmado...")
    os.system("openssl req -new -x509 -keyout server.pem -out server.pem -days 365 -nodes -subj '/CN=localhost'")

server_address = ('0.0.0.0', 8443)
httpd = http.server.HTTPServer(server_address, http.server.SimpleHTTPRequestHandler)

context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain("server.pem")

httpd.socket = context.wrap_socket(httpd.socket, server_side=True)

print(f"Servidor HTTPS corriendo en https://0.0.0.0:8443")
print("NOTA: Deberás aceptar el riesgo en el navegador (Certificado auto-firmado)")
httpd.serve_forever()
