int main() {
    int *a = new int(42);
    int b = *a;
    delete a;
    return b;
}
